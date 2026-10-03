//! Shape paints with independent, stable color and opacity stops.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GradientParam {
    StartX,
    StartY,
    EndX,
    EndY,
    HighlightLength,
    HighlightAngle,
    ColorPosition(u64),
    Red(u64),
    Green(u64),
    Blue(u64),
    ColorMidpoint(u64),
    OpacityPosition(u64),
    Opacity(u64),
    OpacityMidpoint(u64),
}
impl GradientParam {
    pub fn name(self) -> String {
        format!("{self:?}")
    }
    pub fn parse(s: &str) -> Option<Self> {
        use GradientParam::*;
        for p in [StartX, StartY, EndX, EndY, HighlightLength, HighlightAngle] {
            if s == p.name() {
                return Some(p);
            }
        }
        let (name, id) = s.split_once('(')?;
        let id = id.strip_suffix(')')?.parse().ok()?;
        Some(match name {
            "ColorPosition" => ColorPosition(id),
            "Red" => Red(id),
            "Green" => Green(id),
            "Blue" => Blue(id),
            "ColorMidpoint" => ColorMidpoint(id),
            "OpacityPosition" => OpacityPosition(id),
            "Opacity" => Opacity(id),
            "OpacityMidpoint" => OpacityMidpoint(id),
            _ => return None,
        })
    }
    pub fn label(self) -> String {
        use GradientParam::*;
        match self {
            StartX => "Start Point X".into(),
            StartY => "Start Point Y".into(),
            EndX => "End Point X".into(),
            EndY => "End Point Y".into(),
            HighlightLength => "Highlight Length (%)".into(),
            HighlightAngle => "Highlight Angle".into(),
            ColorPosition(id) => format!("Color {id} · Location (%)"),
            Red(id) => format!("Color {id} · Red"),
            Green(id) => format!("Color {id} · Green"),
            Blue(id) => format!("Color {id} · Blue"),
            ColorMidpoint(id) => format!("Color {id} · Midpoint (%)"),
            OpacityPosition(id) => format!("Opacity {id} · Location (%)"),
            Opacity(id) => format!("Opacity {id} (%)"),
            OpacityMidpoint(id) => format!("Opacity {id} · Midpoint (%)"),
        }
    }
    pub fn bounds(self) -> (f64, f64) {
        use GradientParam::*;
        match self {
            Red(_) | Green(_) | Blue(_) => (0., 255.),
            ColorPosition(_) | OpacityPosition(_) | Opacity(_) => (0., 100.),
            ColorMidpoint(_) | OpacityMidpoint(_) => (1., 99.),
            HighlightLength => (-99.9, 99.9),
            _ => (-1000000., 1000000.),
        }
    }
    pub fn stop(self) -> Option<u64> {
        use GradientParam::*;
        match self {
            ColorPosition(id) | Red(id) | Green(id) | Blue(id) | ColorMidpoint(id)
            | OpacityPosition(id) | Opacity(id) | OpacityMidpoint(id) => Some(id),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeGradient {
    pub radial: bool,
    pub colors: Vec<u64>,
    pub opacities: Vec<u64>,
    next_stop: u64,
}
impl Default for ShapeGradient {
    fn default() -> Self {
        Self {
            radial: false,
            colors: vec![1, 2],
            opacities: vec![3, 4],
            next_stop: 5,
        }
    }
}
impl ShapeGradient {
    pub const MAX_STOPS: usize = 32;
    /// Sample a temporary editor gesture without changing tracks or history.
    pub fn preview_edit(
        &self,
        n: &ContentsNode,
        frame: Frame,
        count: usize,
        parameter: GradientParam,
        value: f64,
    ) -> Vec<[f64; 4]> {
        let mut draft = n.clone();
        if value.is_finite()
            && let Some(track) = draft
                .parameters
                .get_mut(&ContentsParam::Gradient(parameter))
        {
            let (min, max) = parameter.bounds();
            *track = AnimatedProperty::new(value.clamp(min, max));
        }
        self.preview(&draft, frame, count)
    }
    pub fn preview(&self, n: &ContentsNode, frame: Frame, count: usize) -> Vec<[f64; 4]> {
        let colors = self.stops(n, frame, false);
        let opacity = self.stops(n, frame, true);
        (0..count.min(256))
            .map(|i| {
                let t = i as f64 / (count.min(256).saturating_sub(1)).max(1) as f64;
                let c = sample(&colors, t, false);
                [c[0], c[1], c[2], sample(&opacity, t, false)[0]]
            })
            .collect()
    }
    pub(crate) fn valid(&self) -> bool {
        let ids = self
            .colors
            .iter()
            .chain(&self.opacities)
            .copied()
            .collect::<BTreeSet<_>>();
        (2..=Self::MAX_STOPS).contains(&self.colors.len())
            && (2..=Self::MAX_STOPS).contains(&self.opacities.len())
            && ids.len() == self.colors.len() + self.opacities.len()
            && !ids.contains(&0)
            && ids.last().is_some_and(|id| *id < self.next_stop)
    }
    pub(crate) fn defaults(&self) -> Vec<(ContentsParam, f64)> {
        use GradientParam::*;
        let mut values = vec![
            (StartX, 0.),
            (StartY, 0.),
            (EndX, 100.),
            (EndY, 0.),
            (HighlightLength, 0.),
            (HighlightAngle, 0.),
        ];
        for (index, &id) in self.colors.iter().enumerate() {
            let value = if index == 0 { 0. } else { 255. };
            values.extend([
                (
                    ColorPosition(id),
                    index as f64 * 100. / (self.colors.len() - 1).max(1) as f64,
                ),
                (Red(id), value),
                (Green(id), value),
                (Blue(id), value),
                (ColorMidpoint(id), 50.),
            ]);
        }
        for (index, &id) in self.opacities.iter().enumerate() {
            values.extend([
                (
                    OpacityPosition(id),
                    index as f64 * 100. / (self.opacities.len() - 1).max(1) as f64,
                ),
                (Opacity(id), 100.),
                (OpacityMidpoint(id), 50.),
            ]);
        }
        values
            .into_iter()
            .map(|(p, v)| (ContentsParam::Gradient(p), v))
            .collect()
    }
    pub fn color_at(&self, n: &ContentsNode, id: u64, frame: Frame) -> Option<u32> {
        self.colors.contains(&id).then(|| {
            [
                GradientParam::Red(id),
                GradientParam::Green(id),
                GradientParam::Blue(id),
            ]
            .into_iter()
            .fold(0, |rgb, p| {
                (rgb << 8) | n.value_at(ContentsParam::Gradient(p), frame).round() as u32
            })
        })
    }
    fn stops(&self, n: &ContentsNode, frame: Frame, opacity: bool) -> Vec<Stop> {
        use GradientParam::*;
        let value = |p| n.value_at(ContentsParam::Gradient(p), frame);
        let ids = if opacity {
            &self.opacities
        } else {
            &self.colors
        };
        let mut stops = ids
            .iter()
            .map(|&id| {
                if opacity {
                    Stop {
                        position: value(OpacityPosition(id)) / 100.,
                        midpoint: value(OpacityMidpoint(id)) / 100.,
                        value: [value(Opacity(id)) / 100.; 3],
                    }
                } else {
                    Stop {
                        position: value(ColorPosition(id)) / 100.,
                        midpoint: value(ColorMidpoint(id)) / 100.,
                        value: [
                            value(Red(id)) / 255.,
                            value(Green(id)) / 255.,
                            value(Blue(id)) / 255.,
                        ],
                    }
                }
            })
            .collect::<Vec<_>>();
        // Stable sort keeps explicit ordering for coincident stops.
        stops.sort_by(|a, b| a.position.total_cmp(&b.position));
        stops
    }
    pub(crate) fn add_stop(
        n: &mut ContentsNode,
        opacity: bool,
        position: f64,
        frame: Frame,
    ) -> Result<(), String> {
        if !position.is_finite() || !(0. ..=100.).contains(&position) {
            return Err("Stop location must be 0–100%".into());
        }
        let gradient = n.kind.gradient().ok_or("Select a gradient paint")?;
        let value = sample(&gradient.stops(n, frame, opacity), position / 100., false);
        let gradient = n.kind.gradient_mut().unwrap();
        let ids = if opacity {
            &mut gradient.opacities
        } else {
            &mut gradient.colors
        };
        if ids.len() >= Self::MAX_STOPS {
            return Err("Gradient has 32 stops".into());
        }
        let id = gradient.next_stop;
        gradient.next_stop = id.checked_add(1).ok_or("Gradient stop ID exhausted")?;
        ids.push(id);
        use GradientParam::*;
        let values = if opacity {
            vec![
                (OpacityPosition(id), position),
                (Opacity(id), value[0] * 100.),
                (OpacityMidpoint(id), 50.),
            ]
        } else {
            vec![
                (ColorPosition(id), position),
                (Red(id), value[0] * 255.),
                (Green(id), value[1] * 255.),
                (Blue(id), value[2] * 255.),
                (ColorMidpoint(id), 50.),
            ]
        };
        for (p, v) in values {
            n.parameters
                .insert(ContentsParam::Gradient(p), AnimatedProperty::new(v));
        }
        Ok(())
    }
    pub(crate) fn remove_stop(n: &mut ContentsNode, id: u64) -> Result<(), String> {
        let g = n.kind.gradient_mut().ok_or("Select a gradient paint")?;
        let ids = if g.colors.contains(&id) {
            &mut g.colors
        } else if g.opacities.contains(&id) {
            &mut g.opacities
        } else {
            return Err("Gradient stop no longer exists".into());
        };
        if ids.len() <= 2 {
            return Err("Keep at least two color and two opacity stops".into());
        }
        ids.retain(|x| *x != id);
        n.parameters
            .retain(|p, _| !matches!(p,ContentsParam::Gradient(p) if p.stop()==Some(id)));
        Ok(())
    }
    pub(crate) fn svg(&self, n: &ContentsNode, frame: Frame, id: &str) -> String {
        use GradientParam::*;
        let v = |p| n.value_at(ContentsParam::Gradient(p), frame);
        let (x, y, ex, ey) = (v(StartX), v(StartY), v(EndX), v(EndY));
        let radius = (ex - x).hypot(ey - y);
        let color = self.stops(n, frame, false);
        let opacity = self.stops(n, frame, true);
        let at = |t, before| {
            let c = sample(&color, t, before);
            [c[0], c[1], c[2], sample(&opacity, t, before)[0]]
        };
        let mut positions = vec![0., 1.];
        positions.extend(color.iter().chain(&opacity).map(|s| s.position));
        positions.sort_by(f64::total_cmp);
        positions.dedup();
        let mut points = vec![(0., at(0., false))];
        for pair in positions.windows(2) {
            subdivide(
                &at,
                pair[0],
                at(pair[0], false),
                pair[1],
                at(pair[1], true),
                0,
                &mut points,
            );
            let after = at(pair[1], false);
            if points.last().unwrap().1 != after {
                points.push((pair[1], after));
            }
        }
        let stops = points
            .into_iter()
            .map(|(t, c)| {
                format!(
                    "<stop offset='{t}' stop-color='rgb({:.6}%,{:.6}%,{:.6}%)' stop-opacity='{}'/>",
                    c[0] * 100.,
                    c[1] * 100.,
                    c[2] * 100.,
                    c[3]
                )
            })
            .collect::<String>();
        if self.radial {
            let angle = (ey - y).atan2(ex - x) + v(HighlightAngle).to_radians();
            let distance = radius * v(HighlightLength) / 100.;
            format!(
                "<radialGradient id='{id}' gradientUnits='userSpaceOnUse' color-interpolation='sRGB' cx='{x}' cy='{y}' r='{radius}' fx='{}' fy='{}'>{stops}</radialGradient>",
                x + distance * angle.cos(),
                y + distance * angle.sin()
            )
        } else {
            format!(
                "<linearGradient id='{id}' gradientUnits='userSpaceOnUse' color-interpolation='sRGB' x1='{x}' y1='{y}' x2='{ex}' y2='{ey}'>{stops}</linearGradient>"
            )
        }
    }
}
struct Stop {
    position: f64,
    midpoint: f64,
    value: [f64; 3],
}
fn sample(stops: &[Stop], t: f64, before: bool) -> [f64; 3] {
    let index = stops.partition_point(|s| {
        if before {
            s.position < t
        } else {
            s.position <= t
        }
    });
    if index == 0 {
        return stops[0].value;
    }
    if index == stops.len() {
        return stops[index - 1].value;
    }
    let (a, b) = (&stops[index - 1], &stops[index]);
    let t = ((t - a.position) / (b.position - a.position))
        .clamp(0., 1.)
        .powf(0.5f64.ln() / a.midpoint.ln());
    std::array::from_fn(|i| a.value[i] + (b.value[i] - a.value[i]) * t)
}
fn subdivide(
    at: &impl Fn(f64, bool) -> [f64; 4],
    a: f64,
    ca: [f64; 4],
    b: f64,
    cb: [f64; 4],
    depth: u8,
    out: &mut Vec<(f64, [f64; 4])>,
) {
    let split = depth < 32
        && [0.25, 0.5, 0.75].into_iter().any(|t| {
            let actual = at(a + (b - a) * t, false);
            (0..4).any(|i| (actual[i] - (ca[i] + (cb[i] - ca[i]) * t)).abs() > 0.001)
        });
    if split {
        let m = (a + b) * 0.5;
        let cm = at(m, false);
        subdivide(at, a, ca, m, cm, depth + 1, out);
        subdivide(at, m, cm, b, cb, depth + 1, out);
    } else {
        out.push((b, cb));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Shape(Shape::default()),
            width: 160.,
            height: 100.,
            name: "Gradient".into(),
        })
        .unwrap();
        e.execute(Command::SetLayerRange {
            id: 1,
            start: 0,
            end: 120,
        })
        .unwrap();
        edit(&mut e, ContentsEdit::Promote);
        edit(
            &mut e,
            ContentsEdit::Add {
                parent: 1,
                kind: ContentsKind::GradientFill {
                    even_odd: false,
                    gradient: ShapeGradient::default(),
                },
            },
        );
        e
    }
    fn edit(e: &mut Editor, edit: ContentsEdit) {
        e.execute(Command::Contents { id: 1, edit }).unwrap();
    }
    fn node(e: &Editor) -> &ContentsNode {
        let Content::ShapeContents(c) = e.selected_layer().unwrap().content() else {
            panic!()
        };
        c.node(5).unwrap()
    }
    fn value(e: &mut Editor, p: GradientParam, value: f64) {
        edit(
            e,
            ContentsEdit::Track {
                item: 5,
                parameter: ContentsParam::Gradient(p),
                edit: TrackEdit::Value { frame: 0, value },
            },
        );
    }
    #[test]
    fn gradient_ramp_draft_samples_do_not_change_keys_or_persist_invalid_values() {
        let mut e = scene();
        edit(
            &mut e,
            ContentsEdit::Track {
                item: 5,
                parameter: ContentsParam::Gradient(GradientParam::ColorMidpoint(1)),
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            },
        );
        let before = e.project().clone();
        let n = node(&e);
        let g = n.kind.gradient().unwrap();
        let base = g.preview(n, 30, 5);
        let draft = g.preview_edit(n, 30, 5, GradientParam::ColorMidpoint(1), 25.);
        assert_eq!(draft[1], [0.5, 0.5, 0.5, 1.]);
        assert_ne!(draft, base);
        assert_eq!(
            g.preview_edit(n, 30, 5, GradientParam::ColorMidpoint(1), f64::NAN),
            base
        );
        assert_eq!(
            g.preview_edit(n, 30, 5, GradientParam::ColorPosition(999), 30.),
            base
        );
        let alpha = g.preview_edit(n, 30, 5, GradientParam::Opacity(3), -100.);
        assert_eq!(alpha[0], [0., 0., 0., 0.]);
        assert_eq!(alpha[4], [1., 1., 1., 1.]);
        assert_eq!(e.project(), &before);
        assert_eq!(
            Project::from_json(&before.to_json().unwrap()).unwrap(),
            before
        );
    }
    #[test]
    fn gradient_stop_identity_keys_versions_and_history_survive_structural_edits() {
        let mut e = scene();
        assert_eq!(e.project().version, 45);
        edit(
            &mut e,
            ContentsEdit::AddGradientStop {
                item: 5,
                opacity: false,
                position: 25.,
                frame: 0,
            },
        );
        assert_eq!(node(&e).kind.gradient().unwrap().colors, vec![1, 2, 5]);
        assert_eq!(
            node(&e).value_at(ContentsParam::Gradient(GradientParam::Red(5)), 0),
            63.75
        );
        let parameter = ContentsParam::Gradient(GradientParam::Red(5));
        for change in [
            TrackEdit::ToggleAnimation { frame: 0 },
            TrackEdit::Value {
                frame: 60,
                value: 255.,
            },
        ] {
            edit(
                &mut e,
                ContentsEdit::Track {
                    item: 5,
                    parameter,
                    edit: change,
                },
            );
        }
        let before = e.project().clone();
        edit(
            &mut e,
            ContentsEdit::RemoveGradientStop { item: 5, stop: 5 },
        );
        assert!(!node(&e).parameters.contains_key(&parameter));
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        e.undo();
        edit(
            &mut e,
            ContentsEdit::GradientType {
                item: 5,
                radial: true,
            },
        );
        assert_eq!(
            node(&e).parameters,
            match &before.composition.layers[0].content {
                Content::ShapeContents(c) => c.node(5).unwrap().parameters.clone(),
                _ => panic!(),
            }
        );
        let path = PropertyPath::Contents { item: 5, parameter };
        let key = e.selected_layer().unwrap().copy_key(path, 60).unwrap();
        e.execute(Command::PasteKeys {
            keys: vec![key],
            frame: 90,
            target: None,
        })
        .unwrap();
        e.execute(Command::ShiftLayer { id: 1, delta: 10 }).unwrap();
        assert_eq!(
            e.selected_layer().unwrap().track_value(path, 40),
            Some(159.375)
        );
        assert!(
            e.selected_layer()
                .unwrap()
                .track(path)
                .unwrap()
                .keys()
                .contains_key(&100)
        );
        let saved = e.project().clone();
        assert_eq!(
            Project::from_json(&saved.to_json().unwrap()).unwrap(),
            saved
        );
        let mut old = saved.clone();
        old.version = 44;
        assert!(Project::from_json(&serde_json::to_string(&old).unwrap()).is_err());
        for _ in 0..3 {
            e.undo();
        }
        assert_eq!(e.project(), &before);
        edit(
            &mut e,
            ContentsEdit::AddGradientStop {
                item: 5,
                opacity: true,
                position: 75.,
                frame: 30,
            },
        );
        assert_eq!(node(&e).kind.gradient().unwrap().opacities, vec![3, 4, 6]);
    }
    #[test]
    fn gradient_midpoints_opacity_and_coincident_stops_are_independent() {
        use GradientParam::*;
        let mut e = scene();
        value(&mut e, ColorMidpoint(1), 25.);
        value(&mut e, Opacity(3), 0.);
        let n = node(&e);
        let preview = n.kind.gradient().unwrap().preview(n, 0, 5);
        assert_eq!(preview[1], [0.5, 0.5, 0.5, 0.25]);
        edit(
            &mut e,
            ContentsEdit::AddGradientStop {
                item: 5,
                opacity: false,
                position: 50.,
                frame: 0,
            },
        );
        edit(
            &mut e,
            ContentsEdit::AddGradientStop {
                item: 5,
                opacity: false,
                position: 50.,
                frame: 0,
            },
        );
        for p in [Red(5), Green(5), Blue(5)] {
            value(&mut e, p, 0.);
        }
        for p in [Red(6), Green(6), Blue(6)] {
            value(&mut e, p, 255.);
        }
        let n = node(&e);
        let gradient = n.kind.gradient().unwrap();
        let stops = gradient.stops(n, 0, false);
        assert_eq!(sample(&stops, 0.5, true), [0.; 3]);
        assert_eq!(sample(&stops, 0.5, false), [1.; 3]);
        assert_eq!(gradient.preview(n, 0, 3)[1], [1., 1., 1., 0.5]);
        // Moving a stop across another changes spatial order without renumbering.
        value(&mut e, ColorPosition(6), 10.);
        assert_eq!(node(&e).kind.gradient().unwrap().colors, vec![1, 2, 5, 6]);
        assert_eq!(
            Project::from_json(&e.project().to_json().unwrap()).unwrap(),
            *e.project()
        );
    }
    #[test]
    fn gradient_limits_and_locked_or_invalid_commands_are_atomic() {
        let mut e = scene();
        let original = e.project().clone();
        for bad in [
            ContentsEdit::RemoveGradientStop { item: 5, stop: 1 },
            ContentsEdit::RemoveGradientStop { item: 5, stop: 999 },
            ContentsEdit::AddGradientStop {
                item: 5,
                opacity: false,
                position: f64::NAN,
                frame: 0,
            },
            ContentsEdit::AddGradientStop {
                item: 5,
                opacity: true,
                position: 101.,
                frame: 0,
            },
            ContentsEdit::AddGradientStop {
                item: 5,
                opacity: true,
                position: 50.,
                frame: u32::MAX,
            },
            ContentsEdit::Track {
                item: 5,
                parameter: ContentsParam::Gradient(GradientParam::Red(999)),
                edit: TrackEdit::Value {
                    frame: 0,
                    value: 10.,
                },
            },
        ] {
            assert!(e.execute(Command::Contents { id: 1, edit: bad }).is_err());
            assert_eq!(e.project(), &original);
        }
        for opacity in [false, true] {
            for _ in 2..ShapeGradient::MAX_STOPS {
                edit(
                    &mut e,
                    ContentsEdit::AddGradientStop {
                        item: 5,
                        opacity,
                        position: 50.,
                        frame: 0,
                    },
                );
            }
            let full = e.project().clone();
            assert!(
                e.execute(Command::Contents {
                    id: 1,
                    edit: ContentsEdit::AddGradientStop {
                        item: 5,
                        opacity,
                        position: 50.,
                        frame: 0
                    }
                })
                .is_err()
            );
            assert_eq!(e.project(), &full);
        }
        e.execute(Command::ToggleLocked(1)).unwrap();
        let locked = e.project().clone();
        assert!(
            e.execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::GradientType {
                    item: 5,
                    radial: true
                }
            })
            .is_err()
        );
        assert_eq!(e.project(), &locked);
        let mut malformed = original;
        let Content::ShapeContents(c) = &mut malformed.composition.layers[0].content else {
            panic!()
        };
        let g = c.node_mut(5).unwrap().kind.gradient_mut().unwrap();
        g.colors[1] = g.colors[0];
        assert!(Project::from_json(&serde_json::to_string(&malformed).unwrap()).is_err());
    }
}
