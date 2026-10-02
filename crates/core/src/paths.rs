use serde::{Deserialize, Serialize};

/// Tangents are offsets from the vertex, in layer coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathVertex {
    pub position: [f64; 2],
    pub incoming: [f64; 2],
    pub outgoing: [f64; 2],
}
impl PathVertex {
    pub fn corner(position: [f64; 2]) -> Self {
        Self {
            position,
            incoming: [0.0; 2],
            outgoing: [0.0; 2],
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VectorPath {
    pub vertices: Vec<PathVertex>,
    pub closed: bool,
}
impl VectorPath {
    pub fn valid(&self) -> bool {
        (if self.closed { 3 } else { 2 }..=1024).contains(&self.vertices.len())
            && self.vertices.iter().all(|v| {
                v.position
                    .iter()
                    .chain(&v.incoming)
                    .chain(&v.outgoing)
                    .all(|x| x.is_finite() && x.abs() <= 1_000_000.0)
            })
    }
    pub fn svg_data(&self) -> String {
        let Some(first) = self.vertices.first() else {
            return String::new();
        };
        let mut data = format!("M{} {}", first.position[0], first.position[1]);
        for i in 1..self.vertices.len() + usize::from(self.closed) {
            let a = &self.vertices[i - 1];
            let b = &self.vertices[i % self.vertices.len()];
            data.push_str(&format!(
                " C{} {} {} {} {} {}",
                a.position[0] + a.outgoing[0],
                a.position[1] + a.outgoing[1],
                b.position[0] + b.incoming[0],
                b.position[1] + b.incoming[1],
                b.position[0],
                b.position[1]
            ));
        }
        if self.closed {
            data.push_str(" Z");
        }
        data
    }
    /// Split a cubic without changing its shape (de Casteljau).
    pub fn insert(&mut self, segment: usize, t: f64) -> bool {
        let count = self.vertices.len();
        if count < 2
            || count >= 1024
            || segment >= count - usize::from(!self.closed)
            || !t.is_finite()
            || !(0.0..1.0).contains(&t)
        {
            return false;
        }
        let next = (segment + 1) % count;
        let a = self.vertices[segment];
        let b = self.vertices[next];
        let add = |a: [f64; 2], b: [f64; 2]| [a[0] + b[0], a[1] + b[1]];
        let sub = |a: [f64; 2], b: [f64; 2]| [a[0] - b[0], a[1] - b[1]];
        let lerp = |a: [f64; 2], b: [f64; 2]| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        let ab = lerp(a.position, add(a.position, a.outgoing));
        let bc = lerp(add(a.position, a.outgoing), add(b.position, b.incoming));
        let cd = lerp(add(b.position, b.incoming), b.position);
        let abc = lerp(ab, bc);
        let bcd = lerp(bc, cd);
        let p = lerp(abc, bcd);
        self.vertices[segment].outgoing = sub(ab, a.position);
        self.vertices[next].incoming = sub(cd, b.position);
        self.vertices.insert(
            segment + 1,
            PathVertex {
                position: p,
                incoming: sub(abc, p),
                outgoing: sub(bcd, p),
            },
        );
        true
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathMaskMode {
    #[default]
    Add,
    Subtract,
    Intersect,
    None,
}
impl PathMaskMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Add => "Add",
            Self::Subtract => "Subtract",
            Self::Intersect => "Intersect",
            Self::None => "None",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Add => Self::Subtract,
            Self::Subtract => Self::Intersect,
            Self::Intersect => Self::None,
            Self::None => Self::Add,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathMask {
    pub path: VectorPath,
    pub mode: PathMaskMode,
    pub inverted: bool,
}
impl PathMask {
    pub fn valid(&self) -> bool {
        self.path.closed && self.path.valid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Content, Editor, Project, Shape};
    pub fn triangle() -> VectorPath {
        VectorPath {
            vertices: vec![
                PathVertex::corner([0.0, 0.0]),
                PathVertex::corner([100.0, 0.0]),
                PathVertex::corner([50.0, 100.0]),
            ],
            closed: true,
        }
    }
    #[test]
    fn path_and_mask_roundtrip_atomic_validation_history() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Shape(Shape {
                path: Some(triangle()),
                ..Default::default()
            }),
            width: 100.0,
            height: 100.0,
            name: "Path".into(),
        })
        .unwrap();
        let before = e.project().clone();
        let mask = PathMask {
            path: triangle(),
            mode: PathMaskMode::Subtract,
            inverted: true,
        };
        e.execute(Command::SetPathMasks {
            id: 1,
            masks: vec![mask.clone()],
        })
        .unwrap();
        let saved = e.project().clone();
        assert_eq!(
            Project::from_json(&saved.to_json().unwrap()).unwrap(),
            saved
        );
        e.undo();
        assert_eq!(*e.project(), before);
        e.redo();
        assert_eq!(*e.project(), saved);
        let mut bad = mask;
        bad.path.vertices[0].position[0] = f64::INFINITY;
        assert!(
            e.execute(Command::SetPathMasks {
                id: 1,
                masks: vec![bad]
            })
            .is_err()
        );
        assert_eq!(*e.project(), saved);
        let old = saved
            .to_json()
            .unwrap()
            .replace("\"version\": 29", "\"version\": 28");
        assert!(Project::from_json(&old).is_err());
    }
    #[test]
    fn split_preserves_cubic_and_closing_segment() {
        let mut p = triangle();
        p.vertices[0].outgoing = [0.0, 100.0];
        p.vertices[1].incoming = [0.0, 100.0];
        assert!(p.insert(0, 0.5));
        assert_eq!(p.vertices[1].position, [50.0, 75.0]);
        assert_eq!(p.vertices[0].outgoing, [0.0, 50.0]);
        assert_eq!(p.vertices[2].incoming, [0.0, 50.0]);
        assert!(p.insert(3, 0.5));
        assert_eq!(p.vertices[4].position, [25.0, 50.0]);
        assert!(p.valid());
        assert!(p.svg_data().ends_with(" Z"));
    }
}
