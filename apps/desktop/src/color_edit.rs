//! Color-dialog drafts never mutate the document until accepted.
use libre_effects_core::{Command, Content, Frame, LayerId, Project, Property};
use serde::{Deserialize, Serialize};

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
    BackgroundDraft(u32),
}
impl Target {
    pub fn alpha(self) -> bool {
        matches!(self, Self::Fill(_))
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Fill(_) => "Layer color",
            Self::Stroke(_) => "Stroke color",
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
                            rgb: layer.text_style().stroke_color,
                            opacity: 100.0,
                        },
                        _ => return Err("Select a shape or text layer".into()),
                    },
                    _ => Color {
                        rgb: layer.color(),
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
            Target::Fill(id) => {
                if self.color.rgb != self.original.rgb {
                    commands.push(Command::SetColor {
                        id,
                        color: self.color.rgb,
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
                        let mut style = layer.text_style();
                        style.stroke_color = self.color.rgb;
                        commands.push(Command::SetTextStyle { id, style });
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
}
