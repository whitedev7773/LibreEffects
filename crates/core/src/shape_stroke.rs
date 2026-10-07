use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeCap {
    #[default]
    Butt,
    Round,
    Square,
}
impl StrokeCap {
    pub const ALL: [Self; 3] = [Self::Butt, Self::Round, Self::Square];
    pub fn label(self) -> &'static str {
        match self {
            Self::Butt => "Butt",
            Self::Round => "Round",
            Self::Square => "Projecting",
        }
    }
    fn svg(self) -> &'static str {
        match self {
            Self::Butt => "butt",
            Self::Round => "round",
            Self::Square => "square",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeJoin {
    Miter,
    #[default]
    Round,
    Bevel,
}
impl StrokeJoin {
    pub const ALL: [Self; 3] = [Self::Miter, Self::Round, Self::Bevel];
    pub fn label(self) -> &'static str {
        match self {
            Self::Miter => "Miter",
            Self::Round => "Round",
            Self::Bevel => "Bevel",
        }
    }
    fn svg(self) -> &'static str {
        match self {
            Self::Miter => "miter",
            Self::Round => "round",
            Self::Bevel => "bevel",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShapeStroke {
    pub cap: StrokeCap,
    pub join: StrokeJoin,
    pub miter_limit: f64,
    /// Alternating dash/gap lengths in layer pixels. Odd lists repeat to form an even cycle.
    pub dashes: Vec<f64>,
    pub dash_offset: f64,
}
impl Default for ShapeStroke {
    fn default() -> Self {
        Self {
            cap: StrokeCap::Butt,
            join: StrokeJoin::Round,
            miter_limit: 4.0,
            dashes: Vec::new(),
            dash_offset: 0.0,
        }
    }
}
impl ShapeStroke {
    pub const MAX_DASHES: usize = 16;
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
    pub fn valid(&self) -> bool {
        self.miter_limit.is_finite()
            && (1.0..=1024.0).contains(&self.miter_limit)
            && self.dash_offset.is_finite()
            && (-32768.0..=32768.0).contains(&self.dash_offset)
            && self.dashes.len() <= Self::MAX_DASHES
            && self
                .dashes
                .iter()
                .all(|v| v.is_finite() && (0.0..=8192.0).contains(v))
    }
    pub(super) fn svg(&self) -> String {
        // SVG defines a zero-sum pattern as solid. Keep its editable rows in the file.
        let dash = if self.dashes.iter().all(|v| *v == 0.0) {
            String::new()
        } else {
            format!(
                " stroke-dasharray='{}' stroke-dashoffset='{}'",
                self.dashes
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
                self.dash_offset
            )
        };
        format!(
            "stroke-linecap='{}' stroke-linejoin='{}' stroke-miterlimit='{}'{dash}",
            self.cap.svg(),
            self.join.svg(),
            self.miter_limit
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Content, Editor, Project, Shape};
    #[test]
    fn legacy_defaults_and_style_version_roundtrip_history() {
        let legacy: Shape = serde_json::from_str(r#"{"stroke_width":8}"#).unwrap();
        assert!(legacy.stroke_style.is_default());
        assert!(
            legacy
                .svg(100., 100., 0)
                .contains("stroke-linejoin='round'")
        );
        assert!(
            !serde_json::to_string(&legacy)
                .unwrap()
                .contains("stroke_style")
        );
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Shape(legacy.clone()),
            width: 100.,
            height: 100.,
            name: "Stroke".into(),
        })
        .unwrap();
        let before = e.project().clone();
        for cap in StrokeCap::ALL {
            for join in StrokeJoin::ALL {
                let mut shape = legacy.clone();
                shape.stroke_style = ShapeStroke {
                    cap,
                    join,
                    miter_limit: 8.,
                    dashes: vec![10., 4., 2.],
                    dash_offset: -7.,
                };
                e.execute(Command::SetContent {
                    id: 1,
                    content: Content::Shape(shape),
                })
                .unwrap();
                let saved = e.project().clone();
                let json = saved.to_json().unwrap();
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&json).unwrap()["version"],
                    37
                );
                assert_eq!(Project::from_json(&json).unwrap(), saved);
                let mut old: serde_json::Value = serde_json::from_str(&json).unwrap();
                old["version"] = 36.into();
                assert!(Project::from_json(&old.to_string()).is_err());
                e.undo();
                assert_eq!(e.project(), &before);
                e.redo();
                assert_eq!(e.project(), &saved);
                e.undo();
            }
        }
    }
    #[test]
    fn invalid_style_changes_are_atomic_and_zero_dash_dots_are_allowed() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Shape(Shape::default()),
            width: 100.,
            height: 100.,
            name: "Stroke".into(),
        })
        .unwrap();
        let before = e.project().clone();
        for style in [
            ShapeStroke {
                miter_limit: 0.9,
                ..Default::default()
            },
            ShapeStroke {
                miter_limit: f64::INFINITY,
                ..Default::default()
            },
            ShapeStroke {
                dash_offset: f64::NAN,
                ..Default::default()
            },
            ShapeStroke {
                dash_offset: 32769.,
                ..Default::default()
            },
            ShapeStroke {
                dashes: vec![1.; 17],
                ..Default::default()
            },
            ShapeStroke {
                dashes: vec![1., -2.],
                ..Default::default()
            },
            ShapeStroke {
                dashes: vec![8193.],
                ..Default::default()
            },
        ] {
            assert!(!style.valid());
            assert!(
                e.execute(Command::SetContent {
                    id: 1,
                    content: Content::Shape(Shape {
                        stroke_style: style,
                        ..Default::default()
                    })
                })
                .is_err()
            );
            assert_eq!(e.project(), &before);
        }
        assert!(
            ShapeStroke {
                cap: StrokeCap::Round,
                dashes: vec![0., 20.],
                ..Default::default()
            }
            .valid()
        );
    }
}
