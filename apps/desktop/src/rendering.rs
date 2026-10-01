//! One compositing path for preview, stills and frame sequences.
use base64::{Engine, engine::general_purpose::STANDARD};
use libre_effects_core::{Content, Project, Property};
use std::{io::Cursor, path::Path, sync::Arc};

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
pub(crate) struct Renderer {
    options: resvg::usvg::Options<'static>,
}
/// Flatten straight RGBA over a solid RGB matte, including partially transparent edges.
pub(crate) fn composite_background(pixels: &mut image::RgbaImage, color: u32) {
    let background = [(color >> 16) & 255, (color >> 8) & 255, color & 255];
    for pixel in pixels.pixels_mut() {
        let alpha = pixel[3] as u32;
        for (channel, matte) in pixel.0[..3].iter_mut().zip(background) {
            *channel = ((*channel as u32 * alpha + matte * (255 - alpha) + 127) / 255) as u8;
        }
        pixel[3] = 255;
    }
}
pub(crate) fn validate_images(project: &Project) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for layer in project
        .compositions()
        .into_iter()
        .flat_map(|(_, comp)| comp.layers())
    {
        if let Content::Image { png } = layer.content() {
            if !seen.insert(png.as_ptr() as usize) {
                continue;
            }
            let bytes = STANDARD.decode(png.as_bytes()).map_err(|e| e.to_string())?;
            let mut reader =
                image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
            reader.limits(image_limits());
            reader
                .decode()
                .map_err(|e| format!("Invalid image in {}: {e}", layer.name()))?;
        }
    }
    Ok(())
}
fn image_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    limits
}
impl Renderer {
    pub fn new() -> Self {
        let mut options = resvg::usvg::Options::default();
        Arc::make_mut(&mut options.fontdb)
            .load_font_data(include_bytes!("../assets/fonts/WantedSans-Regular.ttf").to_vec());
        options.font_family = "Wanted Sans".into();
        Self { options }
    }
    pub fn render(
        &self,
        project: &Project,
        frame: u32,
        max_dimension: u32,
    ) -> Result<image::RgbaImage, String> {
        let c = project.composition();
        if frame >= c.duration() {
            return Err("Frame is outside the composition".into());
        }
        let scale = (max_dimension as f64 / c.width().max(c.height()) as f64).min(1.0);
        let width = (c.width() as f64 * scale).round().max(1.0) as u32;
        let height = (c.height() as f64 * scale).round().max(1.0) as u32;
        if width as u64 * height as u64 > 33_554_432 {
            return Err("Rendering supports up to 32 megapixels per frame".into());
        }
        let mut svg = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink' width='{}' height='{}'>",
            c.width(),
            c.height()
        );
        for l in c
            .layers()
            .iter()
            .rev()
            .filter(|l| l.active_at(frame, c.duration()))
        {
            let Some(matrix) = c.world_transform(l.id(), frame) else {
                continue;
            };
            let id = l.id();
            let e = l.effects();
            svg.push_str(&format!(
                "<defs><filter id='fx{id}' x='-100%' y='-100%' width='300%' height='300%'>"
            ));
            if e.blur > 0.0 {
                svg.push_str(&format!("<feGaussianBlur stdDeviation='{}'/>", e.blur));
            }
            if e.grayscale {
                svg.push_str("<feColorMatrix type='saturate' values='0'/>");
            }
            if e.brightness != 1.0 {
                let b = e.brightness;
                svg.push_str(&format!(
                    "<feColorMatrix values='{b} 0 0 0 0 0 {b} 0 0 0 0 0 {b} 0 0 0 0 0 1 0'/>"
                ));
            }
            svg.push_str("</filter>");
            if let Some(m) = l.mask() {
                svg.push_str(&format!("<clipPath id='mask{id}'><path clip-rule='evenodd' d='{}M{} {}h{}v{}h{}z'/></clipPath>", if m.inverted { format!("M0 0h{}v{}h{}z ",l.width(),l.height(),-l.width()) } else { String::new() },m.x,m.y,m.width,m.height,-m.width));
            }
            svg.push_str("</defs>");
            let a = matrix.0;
            svg.push_str(&format!(
                "<g transform='matrix({} {} {} {} {} {})' opacity='{}' {}><g {}>",
                a[0],
                a[1],
                a[2],
                a[3],
                a[4],
                a[5],
                l.property(Property::Opacity)
                    .value_at(frame)
                    .clamp(0.0, 100.0)
                    / 100.0,
                if e != libre_effects_core::Effects::default() {
                    format!("filter='url(#fx{id})'")
                } else {
                    String::new()
                },
                if l.mask().is_some() {
                    format!("clip-path='url(#mask{id})'")
                } else {
                    String::new()
                }
            ));
            let color = format!("#{:06x}", l.color());
            match l.content() {
                Content::Rectangle => svg.push_str(&format!(
                    "<rect width='{}' height='{}' fill='{color}'/>",
                    l.width(),
                    l.height()
                )),
                Content::Text { text, font_size } => {
                    for (line, s) in text.lines().enumerate() {
                        svg.push_str(&format!("<text x='0' y='{}' font-family='Wanted Sans' font-size='{font_size}' fill='{color}' xml:space='preserve'>{}</text>",font_size * (1.0 + 1.2 * line as f64),xml(s)));
                    }
                }
                Content::Image { png } => svg.push_str(&format!(
                    "<image width='{}' height='{}' xlink:href='data:image/png;base64,{png}'/>",
                    l.width(),
                    l.height()
                )),
                Content::Video { path, .. } => {
                    if let Some(seconds) = l.content().video_time(frame, c.fps()) {
                        let png = crate::footage::frame_png(
                            path,
                            seconds,
                            l.width() as u32,
                            l.height() as u32,
                            max_dimension,
                        )?;
                        svg.push_str(&format!("<image width='{}' height='{}' xlink:href='data:image/png;base64,{png}'/>", l.width(), l.height()));
                    }
                }
            }
            svg.push_str("</g></g>");
        }
        svg.push_str("</svg>");
        let tree = resvg::usvg::Tree::from_str(&svg, &self.options).map_err(|e| e.to_string())?;
        let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
            .ok_or("Could not allocate render buffer")?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(
                width as f32 / c.width() as f32,
                height as f32 / c.height() as f32,
            ),
            &mut pixmap.as_mut(),
        );
        let pixels: Vec<u8> = pixmap
            .pixels()
            .iter()
            .flat_map(|p| {
                let p = p.demultiply();
                [p.red(), p.green(), p.blue(), p.alpha()]
            })
            .collect();
        image::RgbaImage::from_raw(width, height, pixels).ok_or("Invalid render buffer".into())
    }
}
pub(crate) fn import_image(path: &Path) -> Result<(Content, u32, u32), String> {
    if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 8 * 1024 * 1024 {
        return Err("Image import limit is 8 MiB".into());
    }
    let mut reader = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    reader.limits(image_limits());
    let image = reader.decode().map_err(|e| e.to_string())?;
    let (w, h) = (image.width(), image.height());
    let mut bytes = Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let png = STANDARD.encode(bytes.into_inner());
    if png.len() > 12 * 1024 * 1024 {
        return Err("Decoded image is too large to embed".into());
    }
    Ok((Content::Image { png: png.into() }, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opaque_background_composites_straight_alpha_without_changing_alpha_exports() {
        let mut e = scene();
        e.execute(Command::SetCompositionBackground(0x2060a0))
            .unwrap();
        e.execute(Command::AddContent {
            content: Content::Rectangle,
            width: 40.0,
            height: 40.0,
            name: "Red".into(),
        })
        .unwrap();
        e.execute(Command::SetColor {
            id: 1,
            color: 0xff0000,
        })
        .unwrap();
        e.execute(Command::SetValue {
            id: 1,
            property: Property::Opacity,
            frame: 0,
            value: 50.0,
        })
        .unwrap();
        let rgba = Renderer::new().render(e.project(), 0, 100).unwrap();
        assert_eq!(rgba.get_pixel(0, 0)[3], 0);
        assert_eq!(rgba.get_pixel(50, 50).0, [255, 0, 0, 128]);
        let mut opaque = rgba.clone();
        composite_background(&mut opaque, e.project().composition().background_color());
        assert_eq!(opaque.get_pixel(0, 0).0, [32, 96, 160, 255]);
        assert_eq!(opaque.get_pixel(50, 50).0, [144, 48, 80, 255]);
        // Verify PNG storage retains the chosen alpha policy as well as RGB.
        for image in [&rgba, &opaque] {
            let mut png = Cursor::new(Vec::new());
            image.write_to(&mut png, image::ImageFormat::Png).unwrap();
            assert_eq!(
                &image::load_from_memory(png.get_ref()).unwrap().to_rgba8(),
                image
            );
        }
        e.execute(Command::AddBackgroundSolid).unwrap();
        let solid = Renderer::new().render(e.project(), 0, 100).unwrap();
        assert_eq!(solid, opaque);
        let mut transparent_rgb = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 255, 0]));
        composite_background(&mut transparent_rgb, 0xffffff);
        assert_eq!(transparent_rgb.get_pixel(0, 0).0, [255; 4]);
    }
    #[test]
    fn lower_third_template_enters_and_exits_on_transparent_frames() {
        let project =
            Project::from_json(include_str!("../../../examples/lower-third.lfe.json")).unwrap();
        let renderer = Renderer::new();
        for frame in [0, 149] {
            assert!(
                renderer
                    .render(&project, frame, 384)
                    .unwrap()
                    .pixels()
                    .all(|p| p[3] == 0)
            );
        }
        let visible = renderer.render(&project, 30, 384).unwrap();
        assert!(visible.pixels().any(|p| p[3] > 200));
        assert_eq!(visible.get_pixel(0, 0)[3], 0);
    }
    use libre_effects_core::{Command, Editor, Effects, Mask};
    fn scene() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Render".into(),
            width: 100,
            height: 100,
            fps: 30,
            duration: 10,
        })
        .unwrap();
        e
    }
    #[test]
    fn renderer_preserves_alpha_color_and_mask_with_effects() {
        let mut e = scene();
        e.execute(Command::AddContent {
            content: Content::Rectangle,
            width: 60.0,
            height: 60.0,
            name: "Box".into(),
        })
        .unwrap();
        e.execute(Command::SetColor {
            id: 1,
            color: 0xff0000,
        })
        .unwrap();
        let renderer = Renderer::new();
        let image = renderer.render(e.project(), 0, 100).unwrap();
        assert_eq!(image.get_pixel(50, 50).0, [255, 0, 0, 255]);
        assert_eq!(image.get_pixel(0, 0).0, [0, 0, 0, 0]);
        e.execute(Command::SetMask {
            id: 1,
            mask: Some(Mask {
                x: 0.0,
                y: 0.0,
                width: 30.0,
                height: 60.0,
                inverted: false,
            }),
        })
        .unwrap();
        let image = renderer.render(e.project(), 0, 100).unwrap();
        assert_eq!(image.get_pixel(60, 50)[3], 0);
        assert_eq!(image.get_pixel(30, 50)[3], 255);
        e.execute(Command::SetEffects {
            id: 1,
            effects: Effects {
                blur: 2.0,
                brightness: 0.5,
                grayscale: true,
            },
        })
        .unwrap();
        let image = renderer.render(e.project(), 0, 100).unwrap();
        let pixel = image.get_pixel(30, 50);
        assert_eq!(pixel[0], pixel[1]);
        assert_eq!(pixel[1], pixel[2]);
        assert!(pixel[0] > 0 && pixel[0] < 128);
    }
    #[test]
    fn embedded_image_and_wanted_sans_text_render() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.png");
        image::RgbaImage::from_pixel(20, 20, image::Rgba([10, 150, 220, 255]))
            .save(&path)
            .unwrap();
        let (content, w, h) = import_image(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let mut e = scene();
        e.execute(Command::AddContent {
            content,
            width: w as f64,
            height: h as f64,
            name: "Embedded".into(),
        })
        .unwrap();
        let r = Renderer::new();
        assert_eq!(
            r.render(e.project(), 0, 100).unwrap().get_pixel(50, 50).0,
            [10, 150, 220, 255]
        );
        e.execute(Command::RemoveLayer(1)).unwrap();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "한글 & A".into(),
                font_size: 20.0,
            },
            width: 90.0,
            height: 30.0,
            name: "Title".into(),
        })
        .unwrap();
        let image = r.render(e.project(), 0, 100).unwrap();
        assert!(image.pixels().filter(|p| p[3] > 0).count() > 40);
    }
}
