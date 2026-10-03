//! Color-dialog drafts never mutate the document until accepted.
use libre_effects_core::{
    Command, Content, ContentsEdit, ContentsParam, Frame, GradientParam, LayerId, Project,
    Property, ShapePaint, TextPaint, TrackEdit,
};
use serde::{Deserialize, Serialize};

/// Binding captured when a panel shows its fields. A seek, replacement or layer
/// switch invalidates pending input even before the next render synchronizes it.
#[derive(Clone)]
pub(crate) struct InputTarget {
    identity: u64,
    origin: std::sync::Arc<Project>,
    revision: u64,
    frame: Frame,
    layer: LayerId,
}
impl InputTarget {
    fn eligible_layer(s: &crate::editor::EditorState) -> Option<LayerId> {
        (!s.playing).then_some(())?;
        s.editor
            .selected_layer()
            .filter(|l| !l.locked())
            .map(|l| l.id())
    }
    pub fn new(s: &crate::editor::EditorState) -> Option<Self> {
        // Resolve eligibility before cloning possibly large project metadata.
        let layer = Self::eligible_layer(s)?;
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Some(Self {
            identity: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            origin: std::sync::Arc::new(s.editor.project().clone()),
            revision: s.document_revision,
            frame: s.frame,
            layer,
        })
    }
    pub fn current(&self, s: &crate::editor::EditorState) -> bool {
        self.revision == s.document_revision
            && self.frame == s.frame
            && Self::eligible_layer(s) == Some(self.layer)
            && self.origin.as_ref() == s.editor.project()
    }
    pub fn refresh(previous: &mut Option<Self>, s: &crate::editor::EditorState) {
        let Some(layer) = Self::eligible_layer(s) else {
            *previous = None;
            return;
        };
        // Compare borrowed metadata once. Rerenders and context-only changes
        // reuse the immutable source; each displayed field keeps its own clone
        // of this small context so hidden drafts can never be retargeted.
        if let Some(previous) = previous.as_mut()
            && previous.origin.as_ref() == s.editor.project()
        {
            previous.revision = s.document_revision;
            previous.frame = s.frame;
            previous.layer = layer;
            return;
        }
        *previous = Self::new(s);
    }
    pub fn binding(&self) -> String {
        format!(
            "{}-{}-{}-{}-{}",
            self.identity,
            self.origin.active_composition_id(),
            self.layer,
            self.revision,
            self.frame
        )
    }
}

/// Character and Inspector use one semantic RGB path. Format-only changes must
/// not materialize an interpolated-frame key or a sparse baseline override.
pub(crate) fn text_hex_command(
    layer: &libre_effects_core::Layer,
    paint: TextPaint,
    frame: Frame,
    text: &str,
) -> Result<Option<Command>, String> {
    let color = crate::ui::parse_hex_color(text).map_err(str::to_owned)?;
    if layer.locked() || !matches!(layer.content(), Content::Text { .. }) {
        return Err("Select an unlocked text layer".into());
    }
    if layer.text_color_at(paint, frame) == Some(color) {
        return Ok(None);
    }
    layer.text_color_command(paint, color, frame).map(Some)
}

/// The viewer uses one endpoint tool for effect gradients and Contents paints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GradientTarget {
    Effect(
        libre_effects_core::CompositionId,
        LayerId,
        libre_effects_core::EffectId,
    ),
    Contents(libre_effects_core::CompositionId, LayerId, u64),
}
impl GradientTarget {
    pub fn composition(self) -> libre_effects_core::CompositionId {
        match self {
            Self::Effect(c, ..) | Self::Contents(c, ..) => c,
        }
    }
    pub fn layer(self) -> LayerId {
        match self {
            Self::Effect(_, l, _) | Self::Contents(_, l, _) => l,
        }
    }
}

pub(crate) fn next_gradient_gesture() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// A transient ramp edit is shared with the viewer, never with save/export/history.
#[derive(Clone)]
pub(crate) struct GradientDraft {
    pub gesture_id: u64,
    origin: std::sync::Arc<Project>,
    revision: u64,
    frame: Frame,
    tool: crate::editor::Tool,
    pub layer: LayerId,
    pub item: u64,
    pub parameter: GradientParam,
    original: f64,
    pub value: f64,
}
impl GradientDraft {
    pub fn new(
        s: &crate::editor::EditorState,
        item: u64,
        parameter: GradientParam,
    ) -> Option<Self> {
        let layer = s.editor.selected_layer().filter(|l| !l.locked())?;
        let Content::ShapeContents(contents) = layer.content() else {
            return None;
        };
        let node = contents.node(item)?;
        node.kind.gradient()?;
        let path = ContentsParam::Gradient(parameter);
        node.parameters.get(&path)?;
        // Match the visible handle and renderer when a legal curve overshoots its bounds.
        let original = node.value_at(path, s.frame);
        let draft = Self {
            gesture_id: next_gradient_gesture(),
            origin: std::sync::Arc::new(s.editor.project().clone()),
            revision: s.document_revision,
            frame: s.frame,
            tool: s.tool,
            layer: layer.id(),
            item,
            parameter,
            original,
            value: original,
        };
        draft.current(s).then_some(draft)
    }
    pub fn current(&self, s: &crate::editor::EditorState) -> bool {
        self.origin.as_ref() == s.editor.project()
            && self.revision == s.document_revision
            && self.frame == s.frame
            && self.tool == s.tool
            && !s.playing
            && s.colors.session.is_none()
            && s.gradient_editor.is_none()
            && s.text_session.is_none()
            && s.editor.selected() == Some(self.layer)
            && s.contents_selection
                == Some((self.origin.active_composition_id(), self.layer, self.item))
    }
    pub fn command(&self) -> Option<Command> {
        let (lo, hi) = self.parameter.bounds();
        (self.value.is_finite()
            && (lo..=hi).contains(&self.value)
            && (self.value - self.original).abs() > 1e-8)
            .then(|| Command::Contents {
                id: self.layer,
                edit: ContentsEdit::Track {
                    item: self.item,
                    parameter: ContentsParam::Gradient(self.parameter),
                    edit: TrackEdit::Value {
                        frame: self.frame,
                        value: self.value,
                    },
                },
            })
    }
    pub fn preview(&self, s: &crate::editor::EditorState) -> Option<Project> {
        if !self.current(s) {
            return None;
        }
        let mut temporary = libre_effects_core::Editor::default();
        temporary
            .replace_project(self.origin.as_ref().clone())
            .ok()?;
        temporary.execute(self.command()?).ok()?;
        Some(temporary.project().clone())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Color {
    pub rgb: u32,
    pub opacity: f64,
}
impl Color {
    pub fn rgba(rgba: [u8; 4]) -> Self {
        Self {
            rgb: (rgba[0] as u32) << 16 | (rgba[1] as u32) << 8 | rgba[2] as u32,
            opacity: rgba[3] as f64 * 100.0 / 255.0,
        }
    }
    fn valid(self) -> bool {
        self.rgb <= 0xffffff && self.opacity.is_finite() && (0.0..=100.0).contains(&self.opacity)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Target {
    Fill(LayerId),
    Stroke(LayerId),
    Shape(LayerId, ShapePaint),
    Contents(LayerId, u64),
    GradientStop(LayerId, u64, u64),
    BackgroundDraft(u32),
}
impl Target {
    pub fn alpha(self) -> bool {
        matches!(
            self,
            Self::Fill(_) | Self::Shape(_, _) | Self::Contents(_, _)
        )
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Fill(_) => "Layer color",
            Self::Stroke(_) => "Stroke color",
            Self::Shape(_, ShapePaint::Fill) => "Shape fill color",
            Self::Shape(_, ShapePaint::Stroke) => "Shape stroke color",
            Self::Contents(_, _) => "Contents paint color",
            Self::GradientStop(..) => "Gradient stop color",
            Self::BackgroundDraft(_) => "Composition background",
        }
    }
}

pub(crate) struct Session {
    pub target: Target,
    origin: Project,
    revision: u64,
    frame: Frame,
    pub original: Color,
    pub color: Color,
    pub hsv: [f64; 3],
    pub picking: bool,
    pub error: String,
}
impl Session {
    pub fn new(
        target: Target,
        project: &Project,
        revision: u64,
        frame: Frame,
    ) -> Result<Self, String> {
        let color = match target {
            Target::GradientStop(id, item, stop) => {
                let layer = project
                    .composition()
                    .layer(id)
                    .ok_or("Layer no longer exists")?;
                if layer.locked() {
                    return Err("Unlock the layer before changing its color".into());
                }
                let Content::ShapeContents(c) = layer.content() else {
                    return Err("Select a Contents shape layer".into());
                };
                let node = c.node(item).ok_or("Gradient paint no longer exists")?;
                let gradient = node.kind.gradient().ok_or("Select a gradient paint")?;
                Color {
                    rgb: gradient
                        .color_at(node, stop, frame)
                        .ok_or("Color stop no longer exists")?,
                    opacity: 100.,
                }
            }
            Target::Contents(id, item) => {
                let layer = project
                    .composition()
                    .layer(id)
                    .ok_or("Layer no longer exists")?;
                if layer.locked() {
                    return Err("Unlock the layer before changing its color".into());
                }
                let Content::ShapeContents(c) = layer.content() else {
                    return Err("Select a Contents shape layer".into());
                };
                let node = c.node(item).ok_or("Contents paint no longer exists")?;
                let paint = node.paint().ok_or("Select a Fill or Stroke")?;
                Color {
                    rgb: node.paint_color_at(frame).unwrap(),
                    opacity: node.value_at(ContentsParam::Shape(paint.opacity()), frame),
                }
            }
            Target::Shape(id, paint) => {
                let layer = project
                    .composition()
                    .layer(id)
                    .ok_or("Layer no longer exists")?;
                if layer.locked() {
                    return Err("Unlock the layer before changing its color".into());
                }
                let Content::Shape(shape) = layer.content() else {
                    return Err("Select a shape layer".into());
                };
                Color {
                    rgb: shape.paint_color_at(paint, layer.color(), frame),
                    opacity: shape.value_at(paint.opacity(), frame, layer.color()),
                }
            }
            Target::BackgroundDraft(rgb) => Color {
                rgb,
                opacity: 100.0,
            },
            Target::Fill(id) | Target::Stroke(id) => {
                let layer = project
                    .composition()
                    .layer(id)
                    .ok_or("Layer no longer exists")?;
                if layer.locked() {
                    return Err("Unlock the layer before changing its color".into());
                }
                match target {
                    Target::Stroke(_) => match layer.content() {
                        Content::Shape(shape) => Color {
                            rgb: shape.stroke_color,
                            opacity: 100.0,
                        },
                        Content::Text { .. } => Color {
                            rgb: layer
                                .text_color_at(TextPaint::Stroke, frame)
                                .ok_or("Select a text layer")?,
                            opacity: 100.0,
                        },
                        _ => return Err("Select a shape or text layer".into()),
                    },
                    _ => Color {
                        rgb: layer
                            .text_color_at(TextPaint::Fill, frame)
                            .unwrap_or_else(|| layer.color()),
                        opacity: layer.property(Property::Opacity).value_at(frame),
                    },
                }
            }
        };
        if !color.valid() {
            return Err("Invalid color".into());
        }
        Ok(Self {
            target,
            origin: project.clone(),
            revision,
            frame,
            original: color,
            color,
            hsv: to_hsv(color.rgb),
            picking: false,
            error: String::new(),
        })
    }
    pub fn validate(&self, project: &Project, revision: u64, frame: Frame) -> Result<(), String> {
        if project != &self.origin || revision != self.revision || frame != self.frame {
            return Err(
                "The document or frame changed. Cancel and reopen the color dialog.".into(),
            );
        }
        if !self.color.valid() {
            return Err("Invalid color".into());
        }
        Ok(())
    }
    pub fn set_color(&mut self, color: Color) {
        if !color.valid() {
            return;
        }
        self.color.rgb = color.rgb;
        if self.target.alpha() {
            self.color.opacity = color.opacity;
        }
        let mut hsv = to_hsv(color.rgb);
        if hsv[1] == 0.0 {
            hsv[0] = self.hsv[0];
        }
        self.hsv = hsv;
        self.error.clear();
    }
    pub fn set_hsv(&mut self, hsv: [f64; 3]) {
        self.hsv = [
            hsv[0].clamp(0.0, 360.0),
            hsv[1].clamp(0.0, 1.0),
            hsv[2].clamp(0.0, 1.0),
        ];
        self.color.rgb = from_hsv(self.hsv);
        self.error.clear();
    }
    pub fn input(&mut self, index: usize, text: &str) -> Result<(), String> {
        let mut color = self.color;
        if index == 0 {
            let s = text.trim().strip_prefix('#').unwrap_or(text.trim());
            if !(s.len() == 6 || self.target.alpha() && s.len() == 8)
                || !s.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(if self.target.alpha() {
                    "Enter RRGGBB or RRGGBBAA"
                } else {
                    "Enter six RGB hex digits"
                }
                .into());
            }
            let v = u32::from_str_radix(s, 16).map_err(|_| "Invalid HEX color")?;
            color.rgb = if s.len() == 8 { v >> 8 } else { v };
            if s.len() == 8 {
                color.opacity = (v & 255) as f64 * 100.0 / 255.0;
            }
        } else if (1..=3).contains(&index) {
            let v = text
                .trim()
                .parse::<u8>()
                .map_err(|_| "RGB values must be integers from 0 to 255")?;
            let shift = (3 - index) * 8;
            color.rgb = color.rgb & !(255 << shift) | (v as u32) << shift;
        } else if index == 4 && self.target.alpha() {
            color.opacity = text
                .trim()
                .parse()
                .map_err(|_| "Enter layer opacity from 0 to 100")?;
        } else {
            return Err("Unknown color field".into());
        }
        if !color.valid() {
            return Err("Layer opacity must be a finite number from 0 to 100".into());
        }
        self.set_color(color);
        Ok(())
    }
    pub fn command(&self) -> Option<Command> {
        let mut commands = Vec::new();
        match self.target {
            Target::GradientStop(id, item, stop) => {
                for (index, p) in [
                    GradientParam::Red(stop),
                    GradientParam::Green(stop),
                    GradientParam::Blue(stop),
                ]
                .into_iter()
                .enumerate()
                {
                    let shift = (2 - index) * 8;
                    let value = (self.color.rgb >> shift) & 255;
                    if value != (self.original.rgb >> shift) & 255 {
                        commands.push(Command::Contents {
                            id,
                            edit: ContentsEdit::Track {
                                item,
                                parameter: ContentsParam::Gradient(p),
                                edit: TrackEdit::Value {
                                    frame: self.frame,
                                    value: value as f64,
                                },
                            },
                        });
                    }
                }
            }
            Target::Contents(id, item) => {
                let Content::ShapeContents(c) = self.origin.composition().layer(id)?.content()
                else {
                    return None;
                };
                let paint = c.node(item)?.paint()?;
                for (index, p) in paint.channels().into_iter().enumerate() {
                    let shift = (2 - index) * 8;
                    let value = (self.color.rgb >> shift) & 255;
                    if value != (self.original.rgb >> shift) & 255 {
                        commands.push(Command::Contents {
                            id,
                            edit: ContentsEdit::Track {
                                item,
                                parameter: ContentsParam::Shape(p),
                                edit: TrackEdit::Value {
                                    frame: self.frame,
                                    value: value as f64,
                                },
                            },
                        });
                    }
                }
                if self.color.opacity != self.original.opacity {
                    commands.push(Command::Contents {
                        id,
                        edit: ContentsEdit::Track {
                            item,
                            parameter: ContentsParam::Shape(paint.opacity()),
                            edit: TrackEdit::Value {
                                frame: self.frame,
                                value: self.color.opacity,
                            },
                        },
                    });
                }
            }
            Target::Shape(id, paint) => {
                if let Some(layer) = self.origin.composition().layer(id) {
                    if self.color.rgb != self.original.rgb {
                        commands.push(
                            layer
                                .shape_color_command(paint, self.color.rgb, self.frame)
                                .ok()?,
                        );
                    }
                    if self.color.opacity != self.original.opacity {
                        commands.push(Command::EditShape {
                            id,
                            parameter: paint.opacity(),
                            edit: TrackEdit::Value {
                                frame: self.frame,
                                value: self.color.opacity,
                            },
                        });
                    }
                }
            }
            Target::Fill(id) => {
                if self.color.rgb != self.original.rgb {
                    let layer = self.origin.composition().layer(id)?;
                    commands.push(if matches!(layer.content(), Content::Text { .. }) {
                        layer
                            .text_color_command(TextPaint::Fill, self.color.rgb, self.frame)
                            .ok()?
                    } else {
                        Command::SetColor {
                            id,
                            color: self.color.rgb,
                        }
                    });
                }
                if self.color.opacity != self.original.opacity {
                    commands.push(Command::SetValue {
                        id,
                        property: Property::Opacity,
                        frame: self.frame,
                        value: self.color.opacity,
                    });
                }
            }
            Target::Stroke(id) if self.color.rgb != self.original.rgb => {
                if let Some(layer) = self.origin.composition().layer(id) {
                    if matches!(layer.content(), Content::Text { .. }) {
                        commands.push(
                            layer
                                .text_color_command(TextPaint::Stroke, self.color.rgb, self.frame)
                                .ok()?,
                        );
                    } else if let Content::Shape(shape) = layer.content() {
                        let mut shape = shape.clone();
                        shape.stroke_color = self.color.rgb;
                        commands.push(Command::SetContent {
                            id,
                            content: Content::Shape(shape),
                        });
                    }
                }
            }
            _ => {}
        }
        (!commands.is_empty()).then_some(Command::Batch(commands))
    }
}

pub(crate) fn to_hsv(rgb: u32) -> [f64; 3] {
    let [r, g, b] = [
        ((rgb >> 16) & 255) as f64 / 255.0,
        ((rgb >> 8) & 255) as f64 / 255.0,
        (rgb & 255) as f64 / 255.0,
    ];
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h * 60.0, if max == 0.0 { 0.0 } else { d / max }, max]
}
pub(crate) fn from_hsv([h, s, v]: [f64; 3]) -> u32 {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let rgb = match (h / 60.0).floor() as u32 % 6 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    rgb.into_iter().fold(0, |acc, n| {
        acc << 8 | ((n + v - c).clamp(0.0, 1.0) * 255.0).round() as u32
    })
}

#[derive(Default)]
pub(crate) struct Workflow {
    pub session: Option<Session>,
    pub recent: Vec<Color>,
    pub background_result: Option<u32>,
    pub serial: u64,
}
impl Workflow {
    pub fn picking(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.picking)
    }
    pub fn remember(&mut self, color: Color) {
        self.recent.retain(|c| *c != color);
        self.recent.insert(0, color);
        self.recent.truncate(12);
    }
    pub fn load(&mut self, path: &std::path::Path) {
        if std::fs::metadata(path).is_ok_and(|m| m.len() <= 4096)
            && let Ok(data) = std::fs::read(path)
            && let Ok(colors) = serde_json::from_slice::<Vec<Color>>(&data)
        {
            self.recent = colors.into_iter().filter(|c| c.valid()).take(12).collect();
        }
    }
    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        crate::project_io::write_bytes(
            path,
            &serde_json::to_vec(&self.recent).map_err(|e| e.to_string())?,
        )
    }
    pub fn path() -> Option<std::path::PathBuf> {
        std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("XDG_STATE_HOME"))
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|p| std::path::PathBuf::from(p).join(".local/state"))
            })
            .map(|p| p.join("LibreEffects/recent-colors.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::Editor;
    #[test]
    fn gradient_ramp_overshoot_press_release_uses_visible_value_without_history() {
        use libre_effects_core::{Bezier, ContentsKind, Interpolation};
        for parameter in [
            GradientParam::ColorPosition(1),
            GradientParam::OpacityPosition(3),
            GradientParam::ColorMidpoint(1),
            GradientParam::OpacityMidpoint(3),
        ] {
            for direction in [-2., 3.] {
                let mut s = crate::editor::EditorState::default();
                s.editor
                    .execute(Command::AddContent {
                        content: Content::Shape(Default::default()),
                        width: 160.,
                        height: 100.,
                        name: "Overshooting gradient".into(),
                    })
                    .unwrap();
                for edit in [
                    ContentsEdit::Promote,
                    ContentsEdit::Add {
                        parent: 1,
                        kind: ContentsKind::GradientFill {
                            even_odd: false,
                            gradient: Default::default(),
                        },
                    },
                ] {
                    s.editor.execute(Command::Contents { id: 1, edit }).unwrap();
                }
                let path = ContentsParam::Gradient(parameter);
                let (lo, hi) = parameter.bounds();
                for edit in [
                    TrackEdit::Value {
                        frame: 0,
                        value: lo,
                    },
                    TrackEdit::ToggleAnimation { frame: 0 },
                    TrackEdit::Value {
                        frame: 40,
                        value: hi,
                    },
                    TrackEdit::Interpolate {
                        frame: 0,
                        interpolation: Interpolation::Bezier(Bezier {
                            x1: 1. / 3.,
                            y1: direction,
                            x2: 2. / 3.,
                            y2: direction,
                        }),
                    },
                ] {
                    s.editor
                        .execute(Command::Contents {
                            id: 1,
                            edit: ContentsEdit::Track {
                                item: 5,
                                parameter: path,
                                edit,
                            },
                        })
                        .unwrap();
                }
                s.editor.clear_history();
                s.frame = 20;
                s.contents_selection = Some((s.editor.project().active_composition_id(), 1, 5));
                let before = s.editor.project().clone();
                let Content::ShapeContents(contents) = s.editor.selected_layer().unwrap().content()
                else {
                    panic!()
                };
                let node = contents.node(5).unwrap();
                let raw = node.parameters[&path].value_at(s.frame);
                let visible = node.value_at(path, s.frame);
                assert!(
                    raw < lo || raw > hi,
                    "fixture must overshoot: {parameter:?} {raw}"
                );
                assert_eq!(visible, if direction < 0. { lo } else { hi });
                assert_eq!(node.parameters[&path].keys().len(), 2);
                let mut draft = GradientDraft::new(&s, 5, parameter).unwrap();
                assert_eq!(draft.original, visible);
                assert_eq!(draft.value, visible);
                assert!(draft.command().is_none());
                // Mouse-up maps the unmoved handle back to its visible clamped value.
                draft.value = visible;
                if let Some(command) = draft.command() {
                    s.editor.execute(command).unwrap();
                }
                assert!(draft.preview(&s).is_none());
                assert_eq!(s.editor.project(), &before);
                assert!(!s.editor.can_undo());
                assert!(!s.editor.can_redo());
                assert!(GradientDraft::new(&s, 5, GradientParam::ColorPosition(999)).is_none());
                // A real drag away from that boundary still creates exactly one reversible edit.
                draft.value = (lo + hi) / 2.;
                s.editor.execute(draft.command().unwrap()).unwrap();
                assert!(s.editor.can_undo());
                s.editor.undo();
                assert_eq!(s.editor.project(), &before);
                assert!(!s.editor.can_undo());
            }
        }
    }

    #[test]
    fn gradient_stop_color_draft_keeps_other_stops_and_opacity_tracks() {
        use libre_effects_core::{ContentsKind, Shape, ShapeGradient};
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Shape(Shape::default()),
            width: 100.,
            height: 100.,
            name: "Gradient".into(),
        })
        .unwrap();
        e.execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Promote,
        })
        .unwrap();
        e.execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Add {
                parent: 1,
                kind: ContentsKind::GradientFill {
                    even_odd: false,
                    gradient: ShapeGradient::default(),
                },
            },
        })
        .unwrap();
        let p = ContentsParam::Gradient(GradientParam::Red(1));
        e.execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Track {
                item: 5,
                parameter: p,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            },
        })
        .unwrap();
        let before = e.project().clone();
        let mut draft = Session::new(Target::GradientStop(1, 5, 1), e.project(), 1, 30).unwrap();
        assert!(!draft.target.alpha());
        assert!(draft.command().is_none());
        assert!(draft.input(4, "20").is_err());
        draft.input(0, "FF0000").unwrap();
        assert_eq!(e.project(), &before);
        e.execute(draft.command().unwrap()).unwrap();
        let Content::ShapeContents(c) = e.selected_layer().unwrap().content() else {
            panic!()
        };
        let Content::ShapeContents(old) = before.composition().layer(1).unwrap().content() else {
            panic!()
        };
        for (key, track) in &old.node(5).unwrap().parameters {
            if *key != p {
                assert_eq!(&c.node(5).unwrap().parameters[key], track);
            }
        }
        assert_eq!(c.node(5).unwrap().value_at(p, 15), 127.5);
        assert!(draft.validate(e.project(), 2, 30).is_err());
        let saved = e.project().clone();
        assert_eq!(
            Project::from_json(&saved.to_json().unwrap()).unwrap(),
            saved
        );
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &saved);
        assert!(Session::new(Target::GradientStop(1, 5, 3), e.project(), 1, 30).is_err());
        e.execute(Command::ToggleLocked(1)).unwrap();
        assert!(Session::new(Target::GradientStop(1, 5, 1), e.project(), 1, 30).is_err());
    }

    #[test]
    fn hex_rgba_and_rgb_inputs_are_atomic_and_preserve_unedited_opacity() {
        let mut e = Editor::default();
        e.execute(Command::AddSolid).unwrap();
        let mut s = Session::new(Target::Fill(1), e.project(), 0, 0).unwrap();
        s.input(0, "#1234ab80").unwrap();
        assert_eq!(s.color, Color::rgba([18, 52, 171, 128]));
        s.input(0, "FFCC00").unwrap();
        assert_eq!(s.color.opacity, 128.0 * 100.0 / 255.0);
        let before = s.color;
        for (index, text) in [
            (0, "abcd"),
            (0, "123456789"),
            (0, "##123456"),
            (1, "256"),
            (2, "-1"),
            (3, "1.5"),
            (4, "NaN"),
            (4, "inf"),
            (4, "101"),
        ] {
            assert!(s.input(index, text).is_err(), "{text}");
            assert_eq!(s.color, before);
        }
        let mut bg = Session::new(Target::BackgroundDraft(0), e.project(), 0, 0).unwrap();
        assert!(bg.input(0, "11223380").is_err());
        bg.set_color(Color::rgba([1, 2, 3, 0]));
        assert_eq!(
            bg.color,
            Color {
                rgb: 0x010203,
                opacity: 100.0
            }
        );
        assert!(bg.command().is_none());
    }

    #[test]
    fn hsv_roundtrips_and_preserves_gray_hue() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let rgb = r << 16 | g << 8 | b;
                    assert_eq!(from_hsv(to_hsv(rgb)), rgb);
                }
            }
        }
        let mut s =
            Session::new(Target::BackgroundDraft(0xff0000), &Project::default(), 0, 0).unwrap();
        s.set_hsv([240.0, 1.0, 1.0]);
        s.input(0, "808080").unwrap();
        assert_eq!(s.hsv[0], 240.0);
    }

    #[test]
    fn draft_applies_one_history_step_and_roundtrips_rendered_rgba() {
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Color QA".into(),
            width: 100,
            height: 100,
            fps: 30,
            duration: 90,
        })
        .unwrap();
        e.execute(Command::AddSolid).unwrap();
        e.execute(Command::ToggleAnimation {
            id: 1,
            property: Property::Opacity,
            frame: 0,
        })
        .unwrap();
        let before = e.project().clone();
        let mut s = Session::new(Target::Fill(1), e.project(), 3, 30).unwrap();
        assert!(s.command().is_none());
        s.input(0, "33669980").unwrap();
        assert_eq!(e.project(), &before); // Cancel can simply drop the draft.
        assert!(s.validate(e.project(), 3, 30).is_ok());
        assert!(s.validate(e.project(), 4, 30).is_err());
        assert!(s.validate(e.project(), 3, 31).is_err());
        e.execute(s.command().unwrap()).unwrap();
        assert!(s.validate(e.project(), 3, 30).is_err());
        let after = e.project().clone();
        let layer = after.composition().layer(1).unwrap();
        assert_eq!(layer.property(Property::Opacity).value_at(0), 100.0);
        assert_eq!(layer.property(Property::Opacity).keys().len(), 2);
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &after);
        let saved = Project::from_json(&after.to_json().unwrap()).unwrap();
        assert_eq!(saved, after);
        let renderer = crate::rendering::Renderer::new();
        let pixels = renderer.render_preview(&saved, 30, 100).unwrap();
        let p = pixels.get_pixel(50, 50).0;
        for (a, b) in p.into_iter().zip([51u8, 102, 153, 128]) {
            assert!(a.abs_diff(b) <= 1, "{p:?}");
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("color.png");
        pixels.save(&path).unwrap();
        assert_eq!(image::open(&path).unwrap().to_rgba8(), pixels);
        let mut matte = pixels;
        crate::rendering::composite_background(&mut matte, 0x102030);
        assert_eq!(matte.get_pixel(50, 50).0[3], 255);
        e.execute(Command::ToggleLocked(1)).unwrap();
        assert!(Session::new(Target::Fill(1), e.project(), 3, 30).is_err());
    }

    #[test]
    fn animated_shape_color_picker_edits_rgb_and_paint_alpha_in_one_undo() {
        use libre_effects_core::{PropertyPath, Shape, ShapeParam};
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Paint colors".into(),
            width: 200,
            height: 200,
            fps: 30,
            duration: 60,
        })
        .unwrap();
        e.execute(Command::AddContent {
            content: Content::Shape(Shape {
                stroke_color: 0x0000ff,
                stroke_width: 20.,
                ..Default::default()
            }),
            width: 100.,
            height: 100.,
            name: "Paint".into(),
        })
        .unwrap();
        e.execute(Command::SetColor {
            id: 1,
            color: 0xff0000,
        })
        .unwrap();
        for (paint, hex) in [
            (ShapePaint::Fill, "0000FF80"),
            (ShapePaint::Stroke, "FF000040"),
        ] {
            e.execute(
                e.selected_layer()
                    .unwrap()
                    .shape_color_animation_command(paint, 0)
                    .unwrap(),
            )
            .unwrap();
            e.execute(Command::EditShape {
                id: 1,
                parameter: paint.opacity(),
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            })
            .unwrap();
            let before = e.project().clone();
            let mut draft = Session::new(Target::Shape(1, paint), e.project(), 0, 40).unwrap();
            draft.input(0, hex).unwrap();
            assert_eq!(e.project(), &before);
            e.execute(draft.command().unwrap()).unwrap();
            let after = e.project().clone();
            e.undo();
            assert_eq!(e.project(), &before);
            e.redo();
            assert_eq!(e.project(), &after);
            assert!(draft.validate(e.project(), 0, 40).is_err());
            assert_eq!(
                e.selected_layer()
                    .unwrap()
                    .property(Property::Opacity)
                    .value_at(40),
                100.
            );
        }
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        let renderer = crate::rendering::Renderer::new();
        for frame in [0, 10, 20, 30, 40] {
            let image = renderer.render(&saved, frame, 200).unwrap();
            assert_eq!(
                image,
                renderer.render_output(&saved, frame, 200, 200).unwrap()
            );
            let t = frame as f64 / 40.;
            let f = [
                (255. * (1. - t)).round() as u8,
                0,
                (255. * t).round() as u8,
                (255. - 127. * t).round() as u8,
            ];
            let s = [
                (255. * t).round() as u8,
                0,
                (255. * (1. - t)).round() as u8,
                (255. - 191. * t).round() as u8,
            ];
            for (point, expected) in [((100, 100), f), ((45, 100), s)] {
                let actual = image.get_pixel(point.0, point.1).0;
                assert!(
                    actual
                        .into_iter()
                        .zip(expected)
                        .all(|(a, b)| a.abs_diff(b) <= 2),
                    "{frame}: {actual:?} != {expected:?}"
                );
            }
            for paint in [ShapePaint::Fill, ShapePaint::Stroke] {
                let draft = Session::new(Target::Shape(1, paint), &saved, 0, frame).unwrap();
                let expected = if paint == ShapePaint::Fill { f } else { s };
                assert_eq!(
                    draft.original.rgb,
                    (expected[0] as u32) << 16 | expected[2] as u32
                );
            }
        }
        assert_eq!(
            saved
                .composition()
                .layer(1)
                .unwrap()
                .track(PropertyPath::Shape(ShapeParam::FillRed))
                .unwrap()
                .keys()
                .len(),
            2
        );
    }

    #[test]
    fn stroke_changes_preserve_shape_and_background_is_only_a_draft() {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::SetContent {
            id: 1,
            content: Content::Shape(Default::default()),
        })
        .unwrap();
        let before = e.project().clone();
        let mut s = Session::new(Target::Stroke(1), e.project(), 0, 0).unwrap();
        s.input(0, "abcdef").unwrap();
        e.execute(s.command().unwrap()).unwrap();
        let Content::Shape(shape) = e.project().composition().layer(1).unwrap().content() else {
            panic!()
        };
        assert_eq!(shape.stroke_color, 0xabcdef);
        e.undo();
        assert_eq!(e.project(), &before);
        let mut s = Session::new(Target::BackgroundDraft(0), e.project(), 0, 0).unwrap();
        s.input(0, "112233").unwrap();
        assert!(s.command().is_none());
        assert_eq!(e.project(), &before);
        e.execute(Command::SetCompositionBackground(s.color.rgb))
            .unwrap();
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        assert_eq!(saved.composition().background_color(), 0x112233);
        e.undo();
        assert_eq!(e.project(), &before);
    }
    #[test]
    fn text_stroke_picker_preserves_fill_and_undo_restores_style() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Title".into(),
                font_size: 72.0,
            },
            width: 500.0,
            height: 100.0,
            name: "Title".into(),
        })
        .unwrap();
        let before = e.project().clone();
        let mut s = Session::new(Target::Stroke(1), e.project(), 0, 0).unwrap();
        assert!(!s.target.alpha());
        s.input(0, "f08020").unwrap();
        assert_eq!(e.project(), &before);
        e.execute(s.command().unwrap()).unwrap();
        let l = e.selected_layer().unwrap();
        assert_eq!(l.text_style().stroke_color, 0xf08020);
        assert!(!l.text_style().stroke_enabled);
        assert_eq!(l.color(), before.composition().layer(1).unwrap().color());
        assert_eq!(
            l.content(),
            before.composition().layer(1).unwrap().content()
        );
        e.undo();
        assert_eq!(e.project(), &before);
    }

    #[test]
    fn recent_colors_are_bounded_deduplicated_and_persistent() {
        let mut workflow = Workflow::default();
        for rgb in 0..20 {
            workflow.remember(Color {
                rgb,
                opacity: 100.0,
            });
        }
        workflow.remember(Color {
            rgb: 15,
            opacity: 100.0,
        });
        assert_eq!(workflow.recent.len(), 12);
        assert_eq!(workflow.recent[0].rgb, 15);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profile/colors.json");
        workflow.save(&path).unwrap();
        let mut loaded = Workflow::default();
        loaded.load(&path);
        assert_eq!(loaded.recent, workflow.recent);
        std::fs::write(
            &path,
            br#"[{"rgb":16777216,"opacity":100},{"rgb":1,"opacity":101},{"rgb":2,"opacity":50}]"#,
        )
        .unwrap();
        loaded.load(&path);
        assert_eq!(
            loaded.recent,
            vec![Color {
                rgb: 2,
                opacity: 50.0
            }]
        );
    }

    #[test]
    fn contents_color_draft_targets_one_paint_and_only_changes_edited_channels() {
        use libre_effects_core::Shape;
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Shape(Shape::default()),
            width: 100.,
            height: 100.,
            name: "Paint".into(),
        })
        .unwrap();
        e.execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Promote,
        })
        .unwrap();
        for (item, paint) in [(4, ShapePaint::Fill), (3, ShapePaint::Stroke)] {
            for p in [paint.channels()[0], paint.opacity()] {
                e.execute(Command::Contents {
                    id: 1,
                    edit: ContentsEdit::Track {
                        item,
                        parameter: ContentsParam::Shape(p),
                        edit: TrackEdit::ToggleAnimation { frame: 0 },
                    },
                })
                .unwrap();
            }
            let before = e.project().clone();
            let mut draft = Session::new(Target::Contents(1, item), e.project(), 1, 30).unwrap();
            assert!(draft.command().is_none());
            let original = draft.color;
            draft
                .input(1, if original.rgb >> 16 == 17 { "18" } else { "17" })
                .unwrap();
            draft.input(4, "50").unwrap();
            assert_eq!(e.project(), &before);
            e.execute(draft.command().unwrap()).unwrap();
            let Content::ShapeContents(c) = e.selected_layer().unwrap().content() else {
                panic!()
            };
            let n = c.node(item).unwrap();
            let other = if item == 4 { 3 } else { 4 };
            let Content::ShapeContents(old) = before.composition().layer(1).unwrap().content()
            else {
                panic!()
            };
            assert_eq!(c.node(other), old.node(other));
            assert_eq!(
                n.value_at(ContentsParam::Shape(paint.opacity()), 15),
                (original.opacity + 50.) / 2.
            );
            for p in &paint.channels()[1..] {
                assert_eq!(
                    n.parameters[&ContentsParam::Shape(*p)],
                    old.node(item).unwrap().parameters[&ContentsParam::Shape(*p)]
                );
            }
            assert!(draft.validate(e.project(), 2, 30).is_err());
            let saved = e.project().clone();
            assert_eq!(
                Project::from_json(&saved.to_json().unwrap()).unwrap(),
                saved
            );
            e.undo();
            assert_eq!(e.project(), &before);
            e.redo();
            assert_eq!(e.project(), &saved);
        }
        assert!(Session::new(Target::Contents(1, 2), e.project(), 0, 0).is_err());
        assert!(Session::new(Target::Contents(1, 999), e.project(), 0, 0).is_err());
        e.execute(Command::ToggleLocked(1)).unwrap();
        assert!(Session::new(Target::Contents(1, 4), e.project(), 0, 0).is_err());
    }
}
#[cfg(test)]
mod text_paint_controls_tests {
    #[test]
    fn text_paint_input_snapshots_reuse_source_for_rerenders_seeks_and_selection_only_changes() {
        let mut state = crate::editor::EditorState::default();
        assert!(super::InputTarget::new(&state).is_none());
        state.editor = scene(true);
        state
            .editor
            .execute(libre_effects_core::Command::AddSolid)
            .unwrap();
        state.editor.select(1);
        let mut source = None;
        super::InputTarget::refresh(&mut source, &state);
        let original = source.as_ref().unwrap().clone();
        for _ in 0..20 {
            super::InputTarget::refresh(&mut source, &state);
            assert!(std::sync::Arc::ptr_eq(
                &original.origin,
                &source.as_ref().unwrap().origin
            ));
            assert_eq!(source.as_ref().unwrap().binding(), original.binding());
        }
        state.frame = 30;
        super::InputTarget::refresh(&mut source, &state);
        assert!(std::sync::Arc::ptr_eq(
            &original.origin,
            &source.as_ref().unwrap().origin
        ));
        assert_ne!(source.as_ref().unwrap().binding(), original.binding());
        assert!(!original.current(&state));
        state.editor.select(2);
        super::InputTarget::refresh(&mut source, &state);
        assert!(std::sync::Arc::ptr_eq(
            &original.origin,
            &source.as_ref().unwrap().origin
        ));
        assert!(source.as_ref().unwrap().current(&state));
        state.document_revision += 1;
        super::InputTarget::refresh(&mut source, &state);
        assert!(std::sync::Arc::ptr_eq(
            &original.origin,
            &source.as_ref().unwrap().origin
        ));
        state
            .editor
            .execute(libre_effects_core::Command::RenameLayer {
                id: 2,
                name: "Changed".into(),
            })
            .unwrap();
        super::InputTarget::refresh(&mut source, &state);
        assert!(!std::sync::Arc::ptr_eq(
            &original.origin,
            &source.as_ref().unwrap().origin
        ));
        state.playing = true;
        super::InputTarget::refresh(&mut source, &state);
        assert!(source.is_none());
        assert!(super::InputTarget::new(&state).is_none());
        state.playing = false;
        state
            .editor
            .execute(libre_effects_core::Command::ToggleLocked(2))
            .unwrap();
        assert!(super::InputTarget::new(&state).is_none());
        state.editor.clear_selection();
        super::InputTarget::refresh(&mut source, &state);
        assert!(source.is_none());
    }

    #[test]
    fn text_paint_hidden_field_keeps_frozen_target_until_its_visible_sync() {
        let mut state = crate::editor::EditorState::default();
        state.editor = scene(true);
        let mut source = None;
        super::InputTarget::refresh(&mut source, &state);
        // All three numeric panels retain a per-field copy at the same time
        // TextField::sync binds its displayed draft. Hidden rows skip both.
        let field = std::rc::Rc::new(std::cell::RefCell::new(source.clone()));
        let old_binding = field.borrow().as_ref().unwrap().binding();
        state.frame = 30;
        super::InputTarget::refresh(&mut source, &state);
        assert!(!field.borrow().as_ref().unwrap().current(&state));
        assert_eq!(field.borrow().as_ref().unwrap().binding(), old_binding);
        assert!(std::sync::Arc::ptr_eq(
            &field.borrow().as_ref().unwrap().origin,
            &source.as_ref().unwrap().origin
        ));
        state
            .editor
            .execute(libre_effects_core::Command::RenameLayer {
                id: 1,
                name: "Updated".into(),
            })
            .unwrap();
        super::InputTarget::refresh(&mut source, &state);
        assert!(!field.borrow().as_ref().unwrap().current(&state));
        assert_eq!(field.borrow().as_ref().unwrap().binding(), old_binding);
        *field.borrow_mut() = source.clone();
        assert!(field.borrow().as_ref().unwrap().current(&state));
        assert_ne!(field.borrow().as_ref().unwrap().binding(), old_binding);
    }

    #[test]
    fn text_paint_field_binding_is_stable_on_rerender_but_changes_for_same_epoch_project_edits() {
        let mut state = crate::editor::EditorState::default();
        state.editor = scene(true);
        let mut target = None;
        super::InputTarget::refresh(&mut target, &state);
        let first = target.as_ref().unwrap().binding();
        super::InputTarget::refresh(&mut target, &state);
        assert_eq!(target.as_ref().unwrap().binding(), first);
        let frozen = target.as_ref().unwrap().clone();
        state
            .editor
            .execute(libre_effects_core::Command::RenameLayer {
                id: 1,
                name: "Changed".into(),
            })
            .unwrap();
        assert!(!frozen.current(&state));
        super::InputTarget::refresh(&mut target, &state);
        assert_ne!(target.as_ref().unwrap().binding(), first);
        assert!(target.as_ref().unwrap().current(&state));
    }

    use super::*;
    use libre_effects_core::{Editor, PropertyPath, TextParam, TextStyle};

    fn scene(animated: bool) -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Title".into(),
                font_size: 48.,
            },
            width: 400.,
            height: 120.,
            name: "Text".into(),
        })
        .unwrap();
        e.execute(Command::SetColor {
            id: 1,
            color: 0x102030,
        })
        .unwrap();
        e.execute(Command::SetTextStyle {
            id: 1,
            style: TextStyle {
                stroke_color: 0x204060,
                stroke_width: 2.,
                stroke_enabled: true,
                ..Default::default()
            },
        })
        .unwrap();
        if animated {
            for (paint, end) in [(TextPaint::Fill, 0x90a0b0), (TextPaint::Stroke, 0xa0c0e0)] {
                let command = e
                    .selected_layer()
                    .unwrap()
                    .text_color_animation_command(paint, 0)
                    .unwrap();
                e.execute(command).unwrap();
                let command = e
                    .selected_layer()
                    .unwrap()
                    .text_color_command(paint, end, 60)
                    .unwrap();
                e.execute(command).unwrap();
            }
        }
        e.clear_history();
        e
    }

    #[test]
    fn text_paint_picker_samples_fill_and_stroke_at_zero_midpoint_and_endpoint() {
        let e = scene(true);
        for (frame, fill, stroke) in [
            (0, 0x102030, 0x204060),
            (30, 0x506070, 0x6080a0),
            (60, 0x90a0b0, 0xa0c0e0),
        ] {
            for (target, expected) in [(Target::Fill(1), fill), (Target::Stroke(1), stroke)] {
                let session = Session::new(target, e.project(), 7, frame).unwrap();
                assert_eq!(session.original.rgb, expected);
                assert_eq!(session.color.rgb, expected);
                assert!(session.command().is_none());
            }
        }
    }

    #[test]
    fn text_paint_cancel_unchanged_accept_and_format_only_hex_preserve_exact_keys_and_history() {
        let mut e = scene(true);
        // Keep a redo branch to prove a no-op does not clear it.
        e.execute(Command::RenameLayer {
            id: 1,
            name: "temporary".into(),
        })
        .unwrap();
        e.undo();
        let before = e.project().clone();
        assert!(!e.can_undo());
        assert!(e.can_redo());
        for (paint, target) in [
            (TextPaint::Fill, Target::Fill(1)),
            (TextPaint::Stroke, Target::Stroke(1)),
        ] {
            let color = e
                .selected_layer()
                .unwrap()
                .text_color_at(paint, 30)
                .unwrap();
            let mut session = Session::new(target, e.project(), 2, 30).unwrap();
            assert!(session.command().is_none());
            for text in [format!("{color:06x}"), format!(" #{color:06X} ")] {
                session.input(0, &text).unwrap();
                assert!(session.command().is_none());
                assert!(
                    text_hex_command(e.selected_layer().unwrap(), paint, 30, &text)
                        .unwrap()
                        .is_none()
                );
            }
            session.input(0, "ff11ee").unwrap();
            assert!(session.command().is_some());
            drop(session); // Cancel leaves exact project, keys, and history untouched.
        }
        assert_eq!(e.project(), &before);
        assert!(!e.can_undo());
        assert!(e.can_redo());
        for p in [TextPaint::Fill, TextPaint::Stroke]
            .into_iter()
            .flat_map(TextPaint::channels)
        {
            assert!(
                !e.selected_layer()
                    .unwrap()
                    .track(PropertyPath::Text(p))
                    .unwrap()
                    .keys()
                    .contains_key(&30)
            );
        }
        e.redo();
        assert_eq!(e.selected_layer().unwrap().name(), "temporary");
    }

    #[test]
    fn text_paint_partial_rgb_picker_seeds_missing_baselines_and_is_one_undo() {
        for (target, paint, parameter, baseline) in [
            (
                Target::Fill(1),
                TextPaint::Fill,
                TextParam::FillRed,
                0x102030,
            ),
            (
                Target::Stroke(1),
                TextPaint::Stroke,
                TextParam::StrokeRed,
                0x204060,
            ),
        ] {
            let mut e = scene(false);
            e.execute(Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            })
            .unwrap();
            e.execute(Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::Value {
                    frame: 60,
                    value: 144.,
                },
            })
            .unwrap();
            e.clear_history();
            let before = e.project().clone();
            let mut session = Session::new(target, e.project(), 4, 30).unwrap();
            session.input(0, "abcdef").unwrap();
            e.execute(session.command().unwrap()).unwrap();
            let after = e.project().clone();
            let layer = e.selected_layer().unwrap();
            assert_eq!(layer.text_color_at(paint, 0), Some(baseline));
            assert_eq!(layer.text_color_at(paint, 30), Some(0xabcdef));
            for p in paint.channels() {
                let keys = layer.track(PropertyPath::Text(p)).unwrap().keys();
                assert!(keys.contains_key(&0));
                assert!(keys.contains_key(&30));
            }
            assert_eq!(
                layer.text_style(),
                before.composition().layer(1).unwrap().text_style()
            );
            assert_eq!(
                layer.color(),
                before.composition().layer(1).unwrap().color()
            );
            e.undo();
            assert_eq!(e.project(), &before);
            assert!(!e.can_undo());
            e.redo();
            assert_eq!(e.project(), &after);
        }
    }

    #[test]
    fn text_paint_fill_alpha_edits_whole_layer_opacity_and_stroke_stays_rgb_only() {
        let mut e = scene(true);
        e.execute(Command::ToggleAnimation {
            id: 1,
            property: Property::Opacity,
            frame: 0,
        })
        .unwrap();
        e.clear_history();
        let before = e.project().clone();
        let mut session = Session::new(Target::Fill(1), e.project(), 5, 30).unwrap();
        session.input(0, "12345680").unwrap();
        e.execute(session.command().unwrap()).unwrap();
        let layer = e.selected_layer().unwrap();
        assert_eq!(layer.text_color_at(TextPaint::Fill, 30), Some(0x123456));
        assert_eq!(layer.text_color_at(TextPaint::Stroke, 30), Some(0x6080a0));
        assert_eq!(
            layer.property(Property::Opacity).value_at(30),
            128. * 100. / 255.
        );
        assert_eq!(layer.property(Property::Opacity).value_at(0), 100.);
        let mut stroke = Session::new(Target::Stroke(1), e.project(), 6, 30).unwrap();
        assert!(!stroke.target.alpha());
        assert!(stroke.input(0, "abcdef80").is_err());
        assert!(stroke.input(4, "10").is_err());
        stroke.set_color(Color {
            rgb: 0xabcdef,
            opacity: 10.,
        });
        assert_eq!(stroke.color.opacity, 100.);
        e.undo();
        assert_eq!(e.project(), &before);
        assert!(!e.can_undo());
    }

    #[test]
    fn text_paint_targets_reject_locks_bad_hex_missing_layer_and_stale_document_or_frame() {
        let mut e = scene(true);
        let mut session = Session::new(Target::Fill(1), e.project(), 7, 30).unwrap();
        let original = session.color;
        for bad in ["123", "1234567", "##123456", "gg3344"] {
            assert!(session.input(0, bad).is_err());
            assert_eq!(session.color, original);
            assert!(
                text_hex_command(e.selected_layer().unwrap(), TextPaint::Fill, 30, bad).is_err()
            );
        }
        assert!(Session::new(Target::Fill(99), e.project(), 7, 30).is_err());
        assert!(session.validate(e.project(), 8, 30).is_err());
        assert!(session.validate(e.project(), 7, 0).is_err());
        e.execute(Command::ToggleLocked(1)).unwrap();
        assert!(session.validate(e.project(), 7, 30).is_err());
        assert!(Session::new(Target::Stroke(1), e.project(), 7, 30).is_err());
        assert!(
            text_hex_command(e.selected_layer().unwrap(), TextPaint::Fill, 30, "102030").is_err()
        );
    }

    #[test]
    fn text_paint_field_binding_rejects_seek_switch_lock_playback_and_same_project_new_revision() {
        let mut s = crate::editor::EditorState::default();
        s.editor = scene(true);
        s.frame = 30;
        let target = InputTarget::new(&s).unwrap();
        assert!(target.current(&s));
        let old_binding = target.binding();
        s.frame = 60;
        assert!(!target.current(&s));
        assert_ne!(InputTarget::new(&s).unwrap().binding(), old_binding);
        s.frame = 30;
        s.document_revision += 1;
        assert!(!target.current(&s));
        s.document_revision -= 1;
        s.playing = true;
        assert!(!target.current(&s));
        s.playing = false;
        s.editor.execute(Command::AddSolid).unwrap();
        assert!(!target.current(&s));
        s.editor.select(1);
        assert!(!target.current(&s)); // A changed project rejects even at old target.
        s.editor.undo();
        s.editor.select(1);
        assert!(target.current(&s));
        s.editor.execute(Command::ToggleLocked(1)).unwrap();
        assert!(!target.current(&s));
        s.editor.clear_selection();
        assert!(InputTarget::new(&s).is_none());
    }
}
