use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeKind {
    #[default]
    Rectangle,
    RoundedRectangle,
    Ellipse,
    Polygon,
    Star,
}
impl ShapeKind {
    pub const ALL: [Self; 5] = [
        Self::Rectangle,
        Self::RoundedRectangle,
        Self::Ellipse,
        Self::Polygon,
        Self::Star,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::RoundedRectangle => "Rounded Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Polygon => "Polygon",
            Self::Star => "Star",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shape {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<crate::VectorPath>,
    pub kind: ShapeKind,
    pub fill: bool,
    pub stroke_color: u32,
    pub stroke_width: f64,
    pub roundness: f64,
    pub points: u32,
    pub inner_radius: f64,
}
impl Default for Shape {
    fn default() -> Self {
        Self {
            path: None,
            kind: ShapeKind::Rectangle,
            fill: true,
            stroke_color: 0xffffff,
            stroke_width: 0.0,
            roundness: 20.0,
            points: 5,
            inner_radius: 50.0,
        }
    }
}
impl Shape {
    pub fn valid(&self) -> bool {
        self.path.as_ref().is_none_or(crate::VectorPath::valid)
            && self.stroke_color <= 0xffffff
            && self.stroke_width.is_finite()
            && (0.0..=1024.0).contains(&self.stroke_width)
            && self.roundness.is_finite()
            && (0.0..=8192.0).contains(&self.roundness)
            && (3..=128).contains(&self.points)
            && self.inner_radius.is_finite()
            && (0.0..=100.0).contains(&self.inner_radius)
    }
    pub fn svg(&self, width: f64, height: f64, color: u32) -> String {
        let fill = if self.fill {
            format!("#{color:06x}")
        } else {
            "none".into()
        };
        let style = format!(
            "fill='{fill}' stroke='#{:06x}' stroke-width='{}' stroke-linejoin='round'",
            self.stroke_color, self.stroke_width
        );
        if let Some(path) = &self.path {
            return format!("<path d='{}' {style}/>", path.svg_data());
        }
        match self.kind {
            ShapeKind::Rectangle | ShapeKind::RoundedRectangle => format!(
                "<rect width='{width}' height='{height}' rx='{}' {style}/>",
                if self.kind == ShapeKind::RoundedRectangle {
                    self.roundness.min(width / 2.0).min(height / 2.0)
                } else {
                    0.0
                }
            ),
            ShapeKind::Ellipse => format!(
                "<ellipse cx='{}' cy='{}' rx='{}' ry='{}' {style}/>",
                width / 2.0,
                height / 2.0,
                width / 2.0,
                height / 2.0
            ),
            ShapeKind::Polygon | ShapeKind::Star => {
                let star = self.kind == ShapeKind::Star;
                let count = self.points * if star { 2 } else { 1 };
                let points = (0..count)
                    .map(|i| {
                        let angle = std::f64::consts::TAU * i as f64 / count as f64
                            - std::f64::consts::FRAC_PI_2;
                        let radius = if star && i % 2 == 1 {
                            self.inner_radius / 100.0
                        } else {
                            1.0
                        };
                        format!(
                            "{},{}",
                            width / 2.0 + angle.cos() * width / 2.0 * radius,
                            height / 2.0 + angle.sin() * height / 2.0 * radius
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("<polygon points='{points}' {style}/>")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Content, Editor, Project};
    #[test]
    fn shapes_roundtrip_history_and_invalid_edits_are_atomic() {
        for kind in ShapeKind::ALL {
            let shape = Shape {
                kind,
                stroke_width: 5.0,
                ..Default::default()
            };
            let mut e = Editor::default();
            e.execute(Command::AddContent {
                content: Content::Shape(shape.clone()),
                width: 120.0,
                height: 80.0,
                name: kind.label().into(),
            })
            .unwrap();
            let saved = e.project().clone();
            assert_eq!(
                Project::from_json(&saved.to_json().unwrap()).unwrap(),
                saved
            );
            let mut bad = shape.clone();
            bad.points = 0;
            assert!(
                e.execute(Command::SetContent {
                    id: 1,
                    content: Content::Shape(bad)
                })
                .is_err()
            );
            assert_eq!(*e.project(), saved);
            e.undo();
            assert!(e.project().composition().layers().is_empty());
            e.redo();
            assert_eq!(*e.project(), saved);
            assert!(shape.svg(120.0, 80.0, 0x123456).contains("#123456"));
        }
    }
}
