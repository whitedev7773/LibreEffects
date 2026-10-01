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
fn text_svg(text: &str, font_size: f64, color: &str) -> String {
    text.lines().enumerate().map(|(line,s)| format!("<text x='0' y='{}' font-family='Wanted Sans' font-size='{font_size}' fill='{color}' xml:space='preserve'>{}</text>",font_size * (1.0 + 1.2 * line as f64),xml(s))).collect()
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
    fn layers_svg(
        &self,
        project: &Project,
        composition: libre_effects_core::CompositionId,
        frame: u32,
        max_dimension: u32,
        prefix: &str,
        layer_count: &mut usize,
        include_guides: bool,
    ) -> Result<String, String> {
        let c = project
            .composition_by_id(composition)
            .ok_or("Missing source composition")?;
        let mut svg = String::new();
        for l in c.layers().iter().rev().filter(|l| {
            c.layer_active(l, frame, include_guides) && !matches!(l.content(), Content::Null)
        }) {
            let Some(matrix) = c.world_transform(l.id(), frame) else {
                continue;
            };
            *layer_count += 1;
            if *layer_count > 4096 {
                return Err(
                    "Frame exceeds 4096 nested layer instances; simplify the composition".into(),
                );
            }
            let id = format!("{prefix}-{}", l.id());
            let e = l.effects();
            let mut effect_bounds = [0.0, 0.0, l.width(), l.height()];
            if let Content::Text { text, font_size } = l.content()
                && l.effect_stack().iter().any(|e| !e.bypassed())
            {
                // Point text can extend outside the layer's nominal size. Measure the
                // same shaped glyph paths used by the compositor before filtering.
                let source = format!(
                    "<svg xmlns='http://www.w3.org/2000/svg' width='{}' height='{}'>{}</svg>",
                    l.width(),
                    l.height(),
                    text_svg(text, *font_size, "white")
                );
                let measured = resvg::usvg::Tree::from_str(&source, &self.options)
                    .map_err(|e| e.to_string())?;
                let bounds = measured.root().bounding_box();
                let left = f64::from(bounds.left()).min(0.0);
                let top = f64::from(bounds.top()).min(0.0);
                effect_bounds = [
                    left,
                    top,
                    f64::from(bounds.right()).max(l.width()) - left,
                    f64::from(bounds.bottom()).max(l.height()) - top,
                ];
            }
            let (effect_defs, effect_open, effect_close) =
                crate::effect_render::stack(l, frame, &id, effect_bounds)?;
            svg.push_str(&effect_defs);
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
                "<g transform='matrix({} {} {} {} {} {})' opacity='{}'>{effect_open}<g {}><g {}>",
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
                Content::Null => {}
                Content::Rectangle => svg.push_str(&format!(
                    "<rect width='{}' height='{}' fill='{color}'/>",
                    l.width(),
                    l.height()
                )),
                Content::Text { text, font_size } => {
                    svg.push_str(&text_svg(text, *font_size, &color));
                }
                Content::Image { png } => svg.push_str(&format!(
                    "<image width='{}' height='{}' xlink:href='data:image/png;base64,{png}'/>",
                    l.width(),
                    l.height()
                )),
                Content::Composition { composition, .. } => {
                    let source = project
                        .composition_by_id(*composition)
                        .ok_or("Missing source composition")?;
                    if let Some(source_frame) =
                        l.content().composition_frame(frame, c.fps(), source)
                    {
                        let inner = self.layers_svg(
                            project,
                            *composition,
                            source_frame,
                            max_dimension,
                            &id,
                            layer_count,
                            false,
                        )?;
                        // An unchanged full-canvas group already shares its parent's clip.
                        // Avoid a redundant clip pass, which can round antialiased edges again.
                        if source.width() == c.width()
                            && source.height() == c.height()
                            && l.width() == f64::from(source.width())
                            && l.height() == f64::from(source.height())
                            && matrix == libre_effects_core::Affine::default()
                            && e == libre_effects_core::Effects::default()
                        {
                            svg.push_str(&inner);
                        } else {
                            // Nested viewports clip to the source canvas and retain alpha.
                            svg.push_str(&format!("<svg width='{}' height='{}' viewBox='0 0 {} {}' preserveAspectRatio='none' overflow='hidden'>{inner}</svg>", l.width(), l.height(), source.width(), source.height()));
                        }
                    }
                }
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
            svg.push_str(&format!("</g></g>{effect_close}</g>"));
            if svg.len() > 64 * 1024 * 1024 {
                return Err("Frame SVG exceeds 64 MiB; reduce embedded image instances".into());
            }
        }
        Ok(svg)
    }
    pub fn render(
        &self,
        project: &Project,
        frame: u32,
        max_dimension: u32,
    ) -> Result<image::RgbaImage, String> {
        self.render_mode(project, frame, max_dimension, false)
    }
    pub fn render_preview(
        &self,
        project: &Project,
        frame: u32,
        max_dimension: u32,
    ) -> Result<image::RgbaImage, String> {
        self.render_mode(project, frame, max_dimension, true)
    }
    fn render_mode(
        &self,
        project: &Project,
        frame: u32,
        max_dimension: u32,
        include_guides: bool,
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
        svg.push_str(&self.layers_svg(
            project,
            project.active_composition_id(),
            frame,
            max_dimension,
            "root",
            &mut 0,
            include_guides,
        )?);
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
    use libre_effects_core::LayerSwitch;
    #[test]
    fn guides_render_only_in_their_own_preview_and_never_in_nested_output() {
        let mut e = scene();
        e.execute(Command::AddContent {
            content: Content::Rectangle,
            width: 60.0,
            height: 60.0,
            name: "Guide".into(),
        })
        .unwrap();
        e.execute(Command::SetLayerSwitch {
            id: 1,
            switch: LayerSwitch::Guide,
            enabled: true,
        })
        .unwrap();
        let r = Renderer::new();
        assert_eq!(
            r.render_preview(e.project(), 0, 100)
                .unwrap()
                .get_pixel(50, 50)[3],
            255
        );
        assert!(
            r.render(e.project(), 0, 100)
                .unwrap()
                .pixels()
                .all(|p| p[3] == 0)
        );
        e.execute(Command::NewComposition).unwrap();
        e.execute(Command::AddCompositionLayer {
            composition: 1,
            frame: 0,
        })
        .unwrap();
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        for image in [
            r.render(&saved, 0, 100).unwrap(),
            r.render_preview(&saved, 0, 100).unwrap(),
        ] {
            assert!(image.pixels().all(|p| p[3] == 0));
        }
    }
    #[test]
    fn shy_keeps_pixels_solo_filters_pixels_and_null_parents_keep_transforming() {
        let mut e = scene();
        e.execute(Command::AddNull).unwrap();
        let r = Renderer::new();
        assert!(
            r.render(e.project(), 0, 100)
                .unwrap()
                .pixels()
                .all(|p| p[3] == 0)
        );
        e.execute(Command::AddContent {
            content: Content::Rectangle,
            width: 10.0,
            height: 10.0,
            name: "Child".into(),
        })
        .unwrap();
        e.execute(Command::SetParent {
            id: 2,
            parent: Some(1),
            frame: 0,
        })
        .unwrap();
        e.execute(Command::SetValue {
            id: 1,
            property: Property::PositionX,
            frame: 0,
            value: 70.0,
        })
        .unwrap();
        e.execute(Command::ToggleVisible(1)).unwrap();
        e.execute(Command::SetLayerSwitch {
            id: 2,
            switch: LayerSwitch::Shy,
            enabled: true,
        })
        .unwrap();
        e.execute(Command::SetHideShy(true)).unwrap();
        e.execute(Command::AddContent {
            content: Content::Rectangle,
            width: 100.0,
            height: 100.0,
            name: "Cover".into(),
        })
        .unwrap();
        e.execute(Command::SetColor {
            id: 3,
            color: 0xff0000,
        })
        .unwrap();
        assert_eq!(
            r.render(e.project(), 0, 100).unwrap().get_pixel(50, 50).0,
            [255, 0, 0, 255]
        );
        e.execute(Command::SetLayerSwitch {
            id: 2,
            switch: LayerSwitch::Solo,
            enabled: true,
        })
        .unwrap();
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        let export = r.render(&saved, 0, 100).unwrap();
        let preview = r.render_preview(&saved, 0, 100).unwrap();
        assert_eq!(export.get_pixel(50, 50)[3], 0);
        assert_eq!(export.get_pixel(70, 50)[3], 255);
        assert_eq!(export.as_raw(), preview.as_raw());
    }
    #[test]
    fn precompose_and_split_preserve_animated_pixels_after_file_roundtrip() {
        let renderer = Renderer::new();
        for json in [
            include_str!("../../../examples/lower-third.lfe.json"),
            include_str!("../../../examples/content-study.lfe.json"),
            include_str!("../../../examples/precomposition-study.lfe.json"),
        ] {
            let original = Project::from_json(json).unwrap();
            let mut e = Editor::default();
            e.replace_project(original.clone()).unwrap();
            for layer in original
                .composition()
                .layers()
                .iter()
                .filter(|l| l.locked())
            {
                e.execute(Command::ToggleLocked(layer.id())).unwrap();
            }
            e.execute(Command::Precompose {
                layers: original
                    .composition()
                    .layers()
                    .iter()
                    .map(|l| l.id())
                    .collect(),
                name: "Nested".into(),
            })
            .unwrap();
            let id = e.selected().unwrap();
            let duration = original.composition().duration();
            e.execute(Command::SplitLayers {
                ids: vec![id],
                frame: duration / 2,
            })
            .unwrap();
            let nested = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            for frame in [
                0,
                1,
                duration / 4,
                duration / 2 - 1,
                duration / 2,
                duration - 1,
            ] {
                let before = renderer.render(&original, frame, 384).unwrap();
                let after = renderer.render(&nested, frame, 384).unwrap();
                let changed = before
                    .pixels()
                    .zip(after.pixels())
                    .filter(|(a, b)| a != b)
                    .count();
                assert_eq!(changed, 0, "Nested pixels changed at frame {frame}");
            }
        }
    }
    #[test]
    fn nested_instances_sample_different_times_and_ignore_source_background() {
        let mut e = scene();
        e.execute(Command::ConfigureComposition {
            name: "Parent".into(),
            width: 100,
            height: 100,
            fps: 30,
            duration: 60,
        })
        .unwrap();
        e.execute(Command::NewComposition).unwrap();
        e.execute(Command::ConfigureComposition {
            name: "Source".into(),
            width: 100,
            height: 100,
            fps: 24,
            duration: 24,
        })
        .unwrap();
        e.execute(Command::SetCompositionBackground(0x00ff00))
            .unwrap();
        e.execute(Command::AddContent {
            content: Content::Rectangle,
            width: 20.0,
            height: 20.0,
            name: "Animated".into(),
        })
        .unwrap();
        e.execute(Command::ToggleKeyframe {
            id: 1,
            property: Property::Opacity,
            frame: 0,
        })
        .unwrap();
        e.execute(Command::SetValue {
            id: 1,
            property: Property::Opacity,
            frame: 12,
            value: 0.0,
        })
        .unwrap();
        e.activate_composition(1).unwrap();
        e.execute(Command::AddCompositionLayer {
            composition: 2,
            frame: 0,
        })
        .unwrap();
        e.execute(Command::SetPosition {
            id: 2,
            frame: 0,
            x: 25.0,
            y: 50.0,
        })
        .unwrap();
        e.execute(Command::AddCompositionLayer {
            composition: 2,
            frame: 15,
        })
        .unwrap();
        e.execute(Command::SetPosition {
            id: 3,
            frame: 0,
            x: 75.0,
            y: 50.0,
        })
        .unwrap();
        let image = Renderer::new().render(e.project(), 15, 100).unwrap();
        assert_eq!(image.get_pixel(25, 50)[3], 0);
        assert_eq!(image.get_pixel(75, 50)[3], 255);
        assert_eq!(image.get_pixel(0, 0)[3], 0);
    }
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
    fn composition_and_layer_markers_never_change_preview_or_export_pixels() {
        use libre_effects_core::{MarkerEdit, MarkerTarget};
        let mut e = scene();
        e.execute(Command::AddRectangle).unwrap();
        let renderer = Renderer::new();
        let before = renderer.render(e.project(), 3, 100).unwrap();
        for target in [MarkerTarget::Composition, MarkerTarget::Layer(1)] {
            e.execute(Command::Marker {
                target,
                edit: MarkerEdit::Add { frame: 3 },
            })
            .unwrap();
            e.execute(Command::Marker {
                target,
                edit: MarkerEdit::Update {
                    id: 1,
                    frame: 3,
                    duration: 4,
                    name: "No pixels".into(),
                    color: 0xff0000,
                },
            })
            .unwrap();
        }
        let restored =
            libre_effects_core::Project::from_json(&e.project().to_json().unwrap()).unwrap();
        assert_eq!(renderer.render(&restored, 3, 100).unwrap(), before);
        assert_eq!(renderer.render_preview(&restored, 3, 100).unwrap(), before);
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
