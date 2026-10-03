use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapePaint {
    Fill,
    Stroke,
}
impl ShapePaint {
    pub fn from_parameter(p: ShapeParam) -> Option<Self> {
        [Self::Fill, Self::Stroke]
            .into_iter()
            .find(|paint| paint.channels().contains(&p))
    }
    pub fn component_label(p: ShapeParam) -> Option<&'static str> {
        let paint = Self::from_parameter(p)?;
        paint
            .channels()
            .iter()
            .position(|v| *v == p)
            .map(|i| ["R", "G", "B"][i])
    }
    pub fn channels(self) -> [ShapeParam; 3] {
        use ShapeParam::*;
        match self {
            Self::Fill => [FillRed, FillGreen, FillBlue],
            Self::Stroke => [StrokeRed, StrokeGreen, StrokeBlue],
        }
    }
    pub fn opacity(self) -> ShapeParam {
        match self {
            Self::Fill => ShapeParam::FillOpacity,
            Self::Stroke => ShapeParam::StrokeOpacity,
        }
    }
}
impl Shape {
    pub(super) fn has_paint_color_tracks(&self) -> bool {
        [ShapePaint::Fill, ShapePaint::Stroke]
            .into_iter()
            .any(|paint| {
                paint
                    .channels()
                    .iter()
                    .any(|p| self.parameters.contains_key(p))
            })
    }
    pub fn paint_color_animated(&self, paint: ShapePaint) -> bool {
        paint
            .channels()
            .iter()
            .any(|p| self.parameters.get(p).is_some_and(|t| !t.keys.is_empty()))
    }
    pub fn paint_color_at(&self, paint: ShapePaint, fill_color: u32, frame: Frame) -> u32 {
        paint.channels().into_iter().fold(0, |rgb, p| {
            (rgb << 8) | self.value_at(p, frame, fill_color).round() as u32
        })
    }
    pub fn paint_color_animation_command(
        &self,
        id: LayerId,
        paint: ShapePaint,
        frame: Frame,
    ) -> Command {
        let disabling = self.paint_color_animated(paint);
        Command::Batch(
            paint
                .channels()
                .into_iter()
                .filter(|p| {
                    !disabling || self.parameters.get(p).is_some_and(|t| !t.keys.is_empty())
                })
                .map(|parameter| Command::EditShape {
                    id,
                    parameter,
                    edit: TrackEdit::ToggleAnimation { frame },
                })
                .collect(),
        )
    }
}
impl Layer {
    pub fn shape_color_command(
        &self,
        paint: ShapePaint,
        color: u32,
        frame: Frame,
    ) -> Result<Command, String> {
        let Content::Shape(shape) = &self.content else {
            return Err("Select a shape layer".into());
        };
        if self.locked || color > 0xffffff {
            return Err("Invalid color or locked shape layer".into());
        }
        let channels = paint.channels();
        if !channels.iter().any(|p| shape.parameters.contains_key(p)) {
            return Ok(match paint {
                ShapePaint::Fill => Command::SetColor { id: self.id, color },
                ShapePaint::Stroke => {
                    let mut shape = shape.clone();
                    shape.stroke_color = color;
                    Command::SetContent {
                        id: self.id,
                        content: Content::Shape(shape),
                    }
                }
            });
        }
        let first_key = channels
            .iter()
            .filter_map(|p| shape.parameters.get(p)?.keys.keys().next().copied())
            .min();
        let mut commands = Vec::new();
        for (i, parameter) in channels.into_iter().enumerate() {
            // A pasted single-channel key still receives a coherent RGB edit.
            if let Some(first) = first_key {
                if shape
                    .parameters
                    .get(&parameter)
                    .is_none_or(|t| t.keys.is_empty())
                {
                    commands.push(Command::EditShape {
                        id: self.id,
                        parameter,
                        edit: TrackEdit::ToggleAnimation { frame: first },
                    });
                }
            }
            commands.push(Command::EditShape {
                id: self.id,
                parameter,
                edit: TrackEdit::Value {
                    frame,
                    value: ((color >> (16 - 8 * i)) & 255) as f64,
                },
            });
        }
        Ok(Command::Batch(commands))
    }
    pub fn shape_color_animation_command(
        &self,
        paint: ShapePaint,
        frame: Frame,
    ) -> Result<Command, String> {
        let Content::Shape(shape) = &self.content else {
            return Err("Select a shape layer".into());
        };
        if self.locked {
            return Err("Unlock the layer before changing its color".into());
        }
        Ok(shape.paint_color_animation_command(self.id, paint, frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Shape(Shape {
                stroke_width: 20.,
                stroke_color: 0x204060,
                ..Default::default()
            }),
            width: 200.,
            height: 120.,
            name: "Color".into(),
        })
        .unwrap();
        e.execute(Command::SetColor {
            id: 1,
            color: 0x102030,
        })
        .unwrap();
        e
    }
    fn color(e: &Editor, paint: ShapePaint, frame: Frame) -> u32 {
        let l = e.selected_layer().unwrap();
        let Content::Shape(s) = l.content() else {
            unreachable!()
        };
        s.paint_color_at(paint, l.color(), frame)
    }
    fn set(e: &mut Editor, paint: ShapePaint, rgb: u32, frame: Frame) {
        e.execute(
            e.selected_layer()
                .unwrap()
                .shape_color_command(paint, rgb, frame)
                .unwrap(),
        )
        .unwrap();
    }
    fn toggle(e: &mut Editor, paint: ShapePaint, frame: Frame) {
        e.execute(
            e.selected_layer()
                .unwrap()
                .shape_color_animation_command(paint, frame)
                .unwrap(),
        )
        .unwrap();
    }
    #[test]
    fn rgb_animation_is_coherent_one_undo_and_roundtrips_without_changing_base_colors() {
        let mut e = scene();
        for (paint, base, end, middle) in [
            (ShapePaint::Fill, 0x102030, 0x90a0b0, 0x506070),
            (ShapePaint::Stroke, 0x204060, 0x6080a0, 0x406080),
        ] {
            let before = e.project().clone();
            toggle(&mut e, paint, 0);
            let enabled = e.project().clone();
            e.undo();
            assert_eq!(e.project(), &before);
            e.redo();
            assert_eq!(e.project(), &enabled);
            set(&mut e, paint, end, 40);
            let edited = e.project().clone();
            assert_eq!(color(&e, paint, 0), base);
            assert_eq!(color(&e, paint, 20), middle);
            assert_eq!(color(&e, paint, 40), end);
            for p in paint.channels() {
                assert_eq!(
                    e.selected_layer()
                        .unwrap()
                        .track(PropertyPath::Shape(p))
                        .unwrap()
                        .keys()
                        .len(),
                    2
                );
            }
            e.undo();
            assert_eq!(e.project(), &enabled);
            e.redo();
            assert_eq!(e.project(), &edited);
            assert_eq!(
                Project::from_json(&edited.to_json().unwrap()).unwrap(),
                edited
            );
            let mut old: serde_json::Value =
                serde_json::from_str(&edited.to_json().unwrap()).unwrap();
            assert_eq!(old["version"], 41);
            old["version"] = 40.into();
            assert!(Project::from_json(&old.to_string()).is_err());
            toggle(&mut e, paint, 20);
            assert_eq!(color(&e, paint, 100), middle);
            for p in paint.channels() {
                assert!(
                    e.selected_layer()
                        .unwrap()
                        .track(PropertyPath::Shape(p))
                        .unwrap()
                        .keys()
                        .is_empty()
                );
            }
            set(&mut e, paint, 0x778899, 10);
            assert_eq!(color(&e, paint, 100), 0x778899);
        }
        let l = e.selected_layer().unwrap();
        assert_eq!(l.color(), 0x102030);
        assert!(
            matches!(l.content(), Content::Shape(s) if s.stroke_color == 0x204060 && s.stroke_width == 20.)
        );
    }
    #[test]
    fn static_colors_remain_legacy_and_invalid_rgb_edits_are_atomic() {
        let mut e = scene();
        set(&mut e, ShapePaint::Fill, 0xabcdef, 0);
        set(&mut e, ShapePaint::Stroke, 0x654321, 0);
        let saved = e.project().clone();
        assert_eq!(e.selected_layer().unwrap().color(), 0xabcdef);
        assert!(
            matches!(e.selected_layer().unwrap().content(), Content::Shape(s) if s.stroke_color == 0x654321 && s.parameters.is_empty())
        );
        assert_eq!(
            Project::from_json(&saved.to_json().unwrap()).unwrap(),
            saved
        );
        for p in [ShapePaint::Fill, ShapePaint::Stroke] {
            assert!(
                e.selected_layer()
                    .unwrap()
                    .shape_color_command(p, 0x1000000, 0)
                    .is_err()
            );
            assert!(
                e.execute(
                    e.selected_layer()
                        .unwrap()
                        .shape_color_animation_command(p, u32::MAX)
                        .unwrap()
                )
                .is_err()
            );
            assert_eq!(e.project(), &saved);
            for parameter in p.channels() {
                for value in [-1., 256., f64::NAN] {
                    assert!(
                        e.execute(Command::EditShape {
                            id: 1,
                            parameter,
                            edit: TrackEdit::Value { frame: 0, value }
                        })
                        .is_err()
                    );
                    assert_eq!(e.project(), &saved);
                }
            }
        }
        e.execute(Command::ToggleLocked(1)).unwrap();
        assert!(
            e.selected_layer()
                .unwrap()
                .shape_color_command(ShapePaint::Fill, 0, 0)
                .is_err()
        );
        assert!(
            e.selected_layer()
                .unwrap()
                .shape_color_animation_command(ShapePaint::Fill, 0)
                .is_err()
        );
    }
    #[test]
    fn pasted_single_channel_seeds_others_and_rgb_keys_use_common_retiming() {
        let mut e = scene();
        let red = PropertyPath::Shape(ShapeParam::FillRed);
        e.execute(Command::EditTrack {
            id: 1,
            property: red,
            edit: TrackEdit::ToggleKey { frame: 0 },
        })
        .unwrap();
        let before = e.project().clone();
        set(&mut e, ShapePaint::Fill, 0x90a0b0, 40);
        assert_eq!(color(&e, ShapePaint::Fill, 20), 0x506070);
        let copies: Vec<_> = ShapePaint::Fill
            .channels()
            .into_iter()
            .map(|p| {
                e.selected_layer()
                    .unwrap()
                    .copy_key(PropertyPath::Shape(p), 40)
                    .unwrap()
            })
            .collect();
        e.execute(Command::PasteKeys {
            keys: copies,
            frame: 60,
            target: None,
        })
        .unwrap();
        e.execute(Command::MoveKeys {
            keys: ShapePaint::Fill
                .channels()
                .into_iter()
                .map(|p| KeyRef {
                    id: 1,
                    property: PropertyPath::Shape(p),
                    frame: 40,
                })
                .collect(),
            delta: 10,
        })
        .unwrap();
        assert_eq!(color(&e, ShapePaint::Fill, 25), 0x506070);
        e.undo();
        e.undo();
        e.undo();
        assert_eq!(e.project(), &before);
        let svg = match e.selected_layer().unwrap().content() {
            Content::Shape(s) => s.svg_at(200., 120., e.selected_layer().unwrap().color(), 20),
            _ => unreachable!(),
        };
        assert!(svg.contains("fill='#102030'"));
    }
}
