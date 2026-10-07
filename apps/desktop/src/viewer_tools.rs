//! Viewer-only display conversion, layout aids, coordinate mapping and snapping.
use gpui::{App, Bounds, ContentMask, Pixels, Point, TextRun, Window, fill, point, px, rgb, size};
use libre_effects_core::{Guide, GuideAxis};
use serde::{Deserialize, Serialize};

pub const RULER: f32 = 32.0;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Channel {
    #[default]
    Rgb,
    Red,
    Green,
    Blue,
    Alpha,
}
impl Channel {
    pub const ALL: [Self; 5] = [Self::Rgb, Self::Red, Self::Green, Self::Blue, Self::Alpha];
    pub fn label(self) -> &'static str {
        match self {
            Self::Rgb => "RGB",
            Self::Red => "Red",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Alpha => "Alpha",
        }
    }
    pub fn display(self, source: &image::RgbaImage) -> image::RgbaImage {
        let mut image = source.clone();
        for pixel in image.pixels_mut() {
            if self == Self::Rgb {
                pixel.0.swap(0, 2); // GPUI expects BGRA.
            } else {
                let index = match self {
                    Self::Red => 0,
                    Self::Green => 1,
                    Self::Blue => 2,
                    _ => 3,
                };
                let value = pixel[index];
                // Single channels show straight values on an opaque grayscale image.
                *pixel = image::Rgba([value, value, value, 255]);
            }
        }
        image
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewOption {
    Rulers,
    Grid,
    Guides,
    Safe,
    SnapGuides,
    SnapGrid,
    LockGuides,
    GridSize,
}
impl ViewOption {
    pub const ALL: [Self; 8] = [
        Self::Rulers,
        Self::Grid,
        Self::Guides,
        Self::Safe,
        Self::SnapGuides,
        Self::SnapGrid,
        Self::LockGuides,
        Self::GridSize,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Rulers => "Rulers",
            Self::Grid => "Grid",
            Self::Guides => "Guides",
            Self::Safe => "Title / Action Safe",
            Self::SnapGuides => "Snap to guides",
            Self::SnapGrid => "Snap to grid",
            Self::LockGuides => "Lock guides",
            Self::GridSize => "Grid spacing",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ViewerOptions {
    pub rulers: bool,
    pub grid: bool,
    pub guides: bool,
    pub safe: bool,
    pub snap_guides: bool,
    pub snap_grid: bool,
    pub lock_guides: bool,
    pub grid_size: f64,
    pub channel: Channel,
}
impl Default for ViewerOptions {
    fn default() -> Self {
        Self {
            rulers: false,
            grid: false,
            guides: true,
            safe: false,
            snap_guides: true,
            snap_grid: false,
            lock_guides: false,
            grid_size: 100.0,
            channel: Channel::Rgb,
        }
    }
}
impl ViewerOptions {
    pub fn normalize(&mut self) {
        if !self.grid_size.is_finite() {
            self.grid_size = 100.0;
        }
        self.grid_size = self.grid_size.clamp(10.0, 2000.0);
    }
    pub fn enabled(&self, option: ViewOption) -> bool {
        match option {
            ViewOption::Rulers => self.rulers,
            ViewOption::Grid => self.grid,
            ViewOption::Guides => self.guides,
            ViewOption::Safe => self.safe,
            ViewOption::SnapGuides => self.snap_guides,
            ViewOption::SnapGrid => self.snap_grid,
            ViewOption::LockGuides => self.lock_guides,
            ViewOption::GridSize => false,
        }
    }
    pub fn toggle(&mut self, option: ViewOption) {
        let value = match option {
            ViewOption::Rulers => &mut self.rulers,
            ViewOption::Grid => &mut self.grid,
            ViewOption::Guides => &mut self.guides,
            ViewOption::Safe => &mut self.safe,
            ViewOption::SnapGuides => &mut self.snap_guides,
            ViewOption::SnapGrid => &mut self.snap_grid,
            ViewOption::LockGuides => &mut self.lock_guides,
            ViewOption::GridSize => {
                self.grid_size = match self.grid_size as u32 {
                    50 => 100.0,
                    100 => 200.0,
                    _ => 50.0,
                };
                return;
            }
        };
        *value = !*value;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PixelInfo {
    pub position: [u32; 2],
    pub rgba: [u8; 4],
    pub resolution: [u32; 2],
    pub frame: u32,
}
pub(crate) fn sample(
    image: &image::RgbaImage,
    dimensions: [u32; 2],
    p: [f64; 2],
    frame: u32,
) -> Option<PixelInfo> {
    if p.iter().any(|v| !v.is_finite())
        || p[0] < 0.0
        || p[1] < 0.0
        || p[0] >= f64::from(dimensions[0])
        || p[1] >= f64::from(dimensions[1])
        || image.width() == 0
        || image.height() == 0
    {
        return None;
    }
    let x = (p[0] * f64::from(image.width()) / f64::from(dimensions[0])).floor() as u32;
    let y = (p[1] * f64::from(image.height()) / f64::from(dimensions[1])).floor() as u32;
    Some(PixelInfo {
        position: [p[0].floor() as u32, p[1].floor() as u32],
        rgba: image
            .get_pixel(x.min(image.width() - 1), y.min(image.height() - 1))
            .0,
        resolution: [image.width(), image.height()],
        frame,
    })
}

/// Snap a selection as one unit. Distances are checked in logical screen pixels.
pub(crate) fn snap_delta(
    points: &[[f64; 2]],
    mut delta: [f64; 2],
    zoom: f32,
    guides: &[Guide],
    options: &ViewerOptions,
    bypass: bool,
) -> [f64; 2] {
    if bypass || zoom <= 0.0 {
        return delta;
    }
    for axis in 0..2 {
        let mut best = None::<f64>;
        for p in points {
            let value = p[axis] + delta[axis];
            let mut candidates: Vec<f64> = if options.guides && options.snap_guides {
                guides
                    .iter()
                    .filter(|g| (g.axis == GuideAxis::Vertical) == (axis == 0))
                    .map(|g| g.position)
                    .collect()
            } else {
                Vec::new()
            };
            if options.grid && options.snap_grid {
                candidates.push((value / options.grid_size).round() * options.grid_size);
            }
            for target in candidates {
                let correction = target - value;
                if correction.abs() * f64::from(zoom) <= 8.0
                    && best.is_none_or(|b| correction.abs() < b.abs())
                {
                    best = Some(correction);
                }
            }
        }
        if let Some(correction) = best {
            delta[axis] += correction;
        }
    }
    delta
}

fn line(window: &mut Window, a: Point<Pixels>, b: Point<Pixels>, color: u32) {
    window.paint_quad(fill(
        Bounds::new(
            point(a.x.min(b.x), a.y.min(b.y)),
            size(
                (b.x - a.x).abs().max(px(1.0)),
                (b.y - a.y).abs().max(px(1.0)),
            ),
        ),
        rgb(color),
    ));
}
fn label(window: &mut Window, cx: &mut App, position: Point<Pixels>, text: String) {
    let run = TextRun {
        len: text.len(),
        font: window.text_style().font(),
        color: rgb(0xa8a8a8).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let text = window
        .text_system()
        .shape_line(text.into(), px(9.0), &[run], None);
    let _ = text.paint(position, px(13.0), window, cx);
}
fn ticks(min: f64, max: f64, step: f64) -> impl Iterator<Item = f64> {
    let start = (min / step).ceil() as i64;
    let end = (max / step).floor() as i64;
    (start..=end.min(start + 2048)).map(move |i| i as f64 * step)
}
pub(crate) fn paint(
    options: &ViewerOptions,
    guides: &[Guide],
    bounds: Bounds<Pixels>,
    stage: Bounds<Pixels>,
    zoom: f32,
    window: &mut Window,
    cx: &mut App,
) {
    let visible = stage.intersect(&bounds);
    let x0 = f64::from(f32::from(visible.left() - stage.left()) / zoom);
    let y0 = f64::from(f32::from(visible.top() - stage.top()) / zoom);
    let x1 = f64::from(f32::from(visible.right() - stage.left()) / zoom);
    let y1 = f64::from(f32::from(visible.bottom() - stage.top()) / zoom);
    window.with_content_mask(Some(ContentMask { bounds: visible }), |w| {
        if options.grid {
            // Preserve the configured grid while hiding subdivisions smaller than 8 pixels.
            let stride = (8.0 / (options.grid_size * f64::from(zoom)))
                .ceil()
                .max(1.0);
            let step = options.grid_size * stride;
            for x in ticks(x0, x1, step) {
                let x = stage.left() + px(x as f32 * zoom);
                line(
                    w,
                    point(x, visible.top()),
                    point(x, visible.bottom()),
                    0x494949,
                );
            }
            for y in ticks(y0, y1, step) {
                let y = stage.top() + px(y as f32 * zoom);
                line(
                    w,
                    point(visible.left(), y),
                    point(visible.right(), y),
                    0x494949,
                );
            }
        }
        if options.safe {
            for margin in [0.05, 0.10] {
                let a = stage.origin + point(stage.size.width * margin, stage.size.height * margin);
                let b = point(
                    stage.right() - stage.size.width * margin,
                    stage.bottom() - stage.size.height * margin,
                );
                line(w, a, point(b.x, a.y), 0xaaaaaa);
                line(w, point(b.x, a.y), b, 0xaaaaaa);
                line(w, b, point(a.x, b.y), 0xaaaaaa);
                line(w, point(a.x, b.y), a, 0xaaaaaa);
            }
            let center = stage.center();
            line(
                w,
                center - point(px(8.0), px(0.0)),
                center + point(px(8.0), px(0.0)),
                0xaaaaaa,
            );
            line(
                w,
                center - point(px(0.0), px(8.0)),
                center + point(px(0.0), px(8.0)),
                0xaaaaaa,
            );
        }
        if options.guides {
            for guide in guides {
                match guide.axis {
                    GuideAxis::Vertical => {
                        let x = stage.left() + px(guide.position as f32 * zoom);
                        line(
                            w,
                            point(x, visible.top()),
                            point(x, visible.bottom()),
                            0x48c4d8,
                        );
                    }
                    GuideAxis::Horizontal => {
                        let y = stage.top() + px(guide.position as f32 * zoom);
                        line(
                            w,
                            point(visible.left(), y),
                            point(visible.right(), y),
                            0x48c4d8,
                        );
                    }
                }
            }
        }
    });
    if options.rulers {
        let top = Bounds::new(bounds.origin, size(bounds.size.width, px(RULER)));
        let left = Bounds::new(bounds.origin, size(px(RULER), bounds.size.height));
        window.paint_quad(fill(top, rgb(0x292929)));
        window.paint_quad(fill(left, rgb(0x292929)));
        let power = 10.0_f64.powf((65.0 / f64::from(zoom)).log10().floor());
        let major = [1.0, 2.0, 5.0, 10.0]
            .into_iter()
            .map(|n| n * power)
            .find(|n| n * f64::from(zoom) >= 60.0)
            .unwrap_or(power * 10.0);
        window.with_content_mask(Some(ContentMask { bounds: top }), |w| {
            for x in ticks(
                f64::from(f32::from(bounds.left() - stage.left()) / zoom),
                f64::from(f32::from(bounds.right() - stage.left()) / zoom),
                major / 5.0,
            ) {
                let screen = stage.left() + px(x as f32 * zoom);
                let main = (x / major - (x / major).round()).abs() < 1e-6;
                line(
                    w,
                    point(screen, top.bottom() - px(if main { 7.0 } else { 4.0 })),
                    point(screen, top.bottom()),
                    0x8b8b8b,
                );
                if main {
                    label(w, cx, point(screen + px(2.0), top.top()), format!("{x:.0}"));
                }
            }
        });
        window.with_content_mask(Some(ContentMask { bounds: left }), |w| {
            for y in ticks(
                f64::from(f32::from(bounds.top() - stage.top()) / zoom),
                f64::from(f32::from(bounds.bottom() - stage.top()) / zoom),
                major / 5.0,
            ) {
                let screen = stage.top() + px(y as f32 * zoom);
                let main = (y / major - (y / major).round()).abs() < 1e-6;
                line(
                    w,
                    point(left.right() - px(if main { 7.0 } else { 4.0 }), screen),
                    point(left.right(), screen),
                    0x8b8b8b,
                );
                if main {
                    label(
                        w,
                        cx,
                        point(left.left(), screen + px(1.0)),
                        format!("{y:.0}"),
                    );
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guides_and_saved_view_options_never_change_preview_or_export_pixels() {
        use libre_effects_core::{Command, Editor, Property};
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::SetValue {
            id: 1,
            property: Property::Opacity,
            frame: 0,
            value: 40.0,
        })
        .unwrap();
        let renderer = crate::rendering::Renderer::new();
        let before = renderer.render(e.project(), 0, 320).unwrap();
        e.execute(Command::SetGuides(vec![
            Guide {
                axis: GuideAxis::Horizontal,
                position: 75.0,
            },
            Guide {
                axis: GuideAxis::Vertical,
                position: 350.0,
            },
        ]))
        .unwrap();
        let mut views = crate::view_state::ProjectViews::default();
        views.compositions.insert(
            1,
            crate::view_state::CompositionView {
                viewer: ViewerOptions {
                    rulers: true,
                    grid: true,
                    safe: true,
                    channel: Channel::Alpha,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let saved = views.write(e.project()).unwrap();
        let loaded = libre_effects_core::Project::from_json(&saved).unwrap();
        assert_eq!(
            crate::view_state::ProjectViews::read(&saved, &loaded),
            views
        );
        assert_eq!(renderer.render(&loaded, 0, 320).unwrap(), before);
        assert_eq!(renderer.render_preview(&loaded, 0, 320).unwrap(), before);
        e.undo();
        assert!(e.project().composition().guides().is_empty());
        e.redo();
        assert_eq!(renderer.render(e.project(), 0, 320).unwrap(), before);
    }
    #[test]
    fn channels_and_info_use_unmodified_rgba_without_background_or_overlays() {
        let mut image = image::RgbaImage::new(2, 1);
        image.put_pixel(0, 0, image::Rgba([64, 128, 192, 100]));
        image.put_pixel(1, 0, image::Rgba([0, 0, 0, 0]));
        for (channel, expected) in [
            (Channel::Rgb, [192, 128, 64, 100]),
            (Channel::Red, [64, 64, 64, 255]),
            (Channel::Green, [128, 128, 128, 255]),
            (Channel::Blue, [192, 192, 192, 255]),
            (Channel::Alpha, [100, 100, 100, 255]),
        ] {
            assert_eq!(channel.display(&image).get_pixel(0, 0).0, expected);
        }
        assert_eq!(
            sample(&image, [200, 100], [99.9, 50.0], 23).unwrap(),
            PixelInfo {
                position: [99, 50],
                rgba: [64, 128, 192, 100],
                resolution: [2, 1],
                frame: 23
            }
        );
        assert_eq!(
            sample(&image, [200, 100], [100.0, 50.0], 0).unwrap().rgba,
            [0, 0, 0, 0]
        );
        for p in [[-0.1, 0.0], [200.0, 0.0], [0.0, 100.0], [f64::NAN, 0.0]] {
            assert!(sample(&image, [200, 100], p, 0).is_none());
        }
    }
    #[test]
    fn layout_snapping_has_zoom_independent_tolerance_and_alt_bypass() {
        let guides = [
            Guide {
                axis: GuideAxis::Vertical,
                position: 100.0,
            },
            Guide {
                axis: GuideAxis::Horizontal,
                position: 150.0,
            },
        ];
        let mut options = ViewerOptions::default();
        let points = [[20.0, 50.0], [50.0, 70.0]];
        assert_eq!(
            snap_delta(&points, [49.0, 78.0], 2.0, &guides, &options, false),
            [50.0, 80.0]
        );
        assert_eq!(
            snap_delta(&points, [49.0, 78.0], 2.0, &guides, &options, true),
            [49.0, 78.0]
        );
        assert_eq!(
            snap_delta(&[[0.0, 0.0]], [91.0, 0.0], 1.0, &guides, &options, false),
            [91.0, 0.0]
        );
        assert_eq!(
            snap_delta(&[[0.0, 0.0]], [91.0, 0.0], 0.5, &guides, &options, false),
            [100.0, 0.0]
        );
        options.guides = false;
        assert_eq!(
            snap_delta(&points, [49.0, 78.0], 2.0, &guides, &options, false),
            [49.0, 78.0]
        );
        options.grid = true;
        options.snap_grid = true;
        options.grid_size = 50.0;
        assert_eq!(
            snap_delta(&[[0.0, 0.0]], [-48.0, 49.0], 1.0, &[], &options, false),
            [-50.0, 50.0]
        );
    }
}
