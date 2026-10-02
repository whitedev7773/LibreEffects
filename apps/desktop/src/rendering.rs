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
    pub(crate) options: resvg::usvg::Options<'static>,
    decoders: std::sync::Mutex<crate::video_decoder::Pool>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
}
pub(crate) fn text_svg(
    text: &str,
    font_size: f64,
    color: &str,
    width: f64,
    style: libre_effects_core::TextStyle,
) -> String {
    let style = crate::fonts::resolved(&style);
    let (x, anchor) = match style.align {
        libre_effects_core::TextAlign::Left => (0.0, "start"),
        libre_effects_core::TextAlign::Center => (width / 2.0, "middle"),
        libre_effects_core::TextAlign::Right => (width, "end"),
    };
    text.lines().enumerate().map(|(line,s)| format!("<text x='{x}' y='{}' text-anchor='{anchor}' letter-spacing='{}' font-family='{}' font-weight='{}' font-style='{}' font-size='{font_size}' fill='{color}' xml:space='preserve'>{}</text>",font_size * (1.0 + style.leading * line as f64),style.tracking * font_size / 1000.0,xml(crate::fonts::svg_family(&style)),style.weight, if style.italic { "italic" } else { "normal" },xml(s))).collect()
}
fn layer_text_svg(
    text: &str,
    size: f64,
    color: &str,
    width: f64,
    height: f64,
    style: libre_effects_core::TextStyle,
) -> String {
    let fill = if style.fill_enabled {
        text_geometry_svg(text, size, color, width, height, &style)
    } else {
        String::new()
    };
    if !style.stroke_enabled || style.stroke_width == 0.0 {
        return fill;
    }
    let join = match style.stroke_join {
        libre_effects_core::TextStrokeJoin::Miter => "miter",
        libre_effects_core::TextStrokeJoin::Round => "round",
        libre_effects_core::TextStrokeJoin::Bevel => "bevel",
    };
    let stroke = format!(
        "<g stroke='#{:06x}' stroke-width='{}' stroke-linejoin='{join}' stroke-miterlimit='4'>{}</g>",
        style.stroke_color,
        style.stroke_width,
        text_geometry_svg(text, size, "none", width, height, &style)
    );
    if style.stroke_over_fill {
        format!("{fill}{stroke}")
    } else {
        format!("{stroke}{fill}")
    }
}
// Keep paint out of shaping and caret metrics. Separate whole-layer passes also
// preserve the chosen ordering when characters or lines overlap.
fn text_geometry_svg(
    text: &str,
    size: f64,
    color: &str,
    width: f64,
    height: f64,
    style: &libre_effects_core::TextStyle,
) -> String {
    if !style.paragraph {
        return text_svg(text, size, color, width, style.clone());
    }
    let lines = crate::text_flow::lines(text, size, width, style);
    let mut svg = format!("<svg width='{width}' height='{height}' overflow='hidden'>");
    for (i, line) in lines
        .iter()
        .take(crate::text_flow::composed_count(&lines, height))
        .enumerate()
    {
        let y = i as f64 * size * style.leading;
        svg.push_str(&format!(
            "<g transform='translate(0 {y})'>{}</g>",
            text_svg(
                &text[line.range.start..line.visible_end],
                size,
                color,
                width,
                style.clone()
            )
        ));
    }
    svg.push_str("</svg>");
    svg
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
    for (name, content) in project
        .compositions()
        .into_iter()
        .flat_map(|(_, comp)| comp.layers())
        .map(|layer| (layer.name(), layer.content()))
        .chain(
            project
                .asset_library()
                .assets()
                .values()
                .map(|a| (a.name(), a.content())),
        )
    {
        if let Content::Image { png } = content {
            if !seen.insert(png.as_ptr() as usize) {
                continue;
            }
            let bytes = STANDARD.decode(png.as_bytes()).map_err(|e| e.to_string())?;
            let mut reader =
                image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
            reader.limits(image_limits());
            reader
                .decode()
                .map_err(|e| format!("Invalid image in {name}: {e}"))?;
        }
    }
    Ok(())
}
pub(crate) fn image_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    limits
}
fn count_layer(count: &mut usize) -> Result<(), String> {
    *count += 1;
    if *count > 4096 {
        return Err(
            "Frame exceeds 4096 nested layer or matte instances; simplify the composition".into(),
        );
    }
    Ok(())
}
impl Renderer {
    pub fn new() -> Self {
        Self::with_cancel(Default::default())
    }
    pub fn with_cancel(cancel: Arc<std::sync::atomic::AtomicBool>) -> Self {
        let options = crate::fonts::render_options();
        Self {
            options,
            decoders: Default::default(),
            cancel,
        }
    }
    pub fn clear_decoders(&self) {
        self.decoders.lock().unwrap().clear();
    }
    fn check_cancel(&self) -> Result<(), String> {
        crate::video_decoder::check_cancel(&self.cancel)
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
            c.layer_active(l, frame, include_guides)
                && !matches!(l.content(), Content::Null | Content::Audio { .. })
        }) {
            self.check_cancel()?;
            if matches!(l.content(), Content::Adjustment) {
                let Some(matrix) = c.world_transform(l.id(), frame) else {
                    continue;
                };
                count_layer(layer_count)?;
                let id = format!("{prefix}-{}", l.id());
                let matte = self.matte_pixels(
                    project,
                    composition,
                    l,
                    frame,
                    max_dimension,
                    &id,
                    layer_count,
                )?;
                svg = self.adjust_composite(
                    &svg,
                    l,
                    frame,
                    matrix,
                    c.width(),
                    c.height(),
                    max_dimension,
                    &id,
                    matte.as_ref().map(|(p, m)| (p, *m)),
                )?;
            } else {
                let source = self.isolated_layer_svg(
                    project,
                    composition,
                    l.id(),
                    frame,
                    max_dimension,
                    prefix,
                    layer_count,
                )?;
                if l.blend_mode() == libre_effects_core::BlendMode::Normal {
                    svg.push_str(&source);
                } else {
                    svg = self.blend_composite(
                        &svg,
                        &source,
                        l.blend_mode(),
                        c.width(),
                        c.height(),
                        max_dimension,
                    )?;
                }
            }
            if svg.len() > 64 * 1024 * 1024 {
                return Err("Frame SVG exceeds 64 MiB; reduce embedded image instances".into());
            }
        }
        Ok(svg)
    }
    /// A matte reads the source's own effects, mask, opacity and transforms before
    /// blending. Visibility, Solo and Guide only control its independent composite.
    pub(crate) fn isolated_layer_svg(
        &self,
        project: &Project,
        composition: libre_effects_core::CompositionId,
        layer: libre_effects_core::LayerId,
        frame: u32,
        max_dimension: u32,
        prefix: &str,
        layer_count: &mut usize,
    ) -> Result<String, String> {
        let c = project
            .composition_by_id(composition)
            .ok_or("Missing source composition")?;
        let l = c.layer(layer).ok_or("Missing matte source")?;
        if frame < l.in_frame()
            || frame >= l.out_frame(c.duration())
            || matches!(l.content(), Content::Null | Content::Adjustment)
        {
            return Ok(String::new());
        }
        let Some(matrix) = c.world_transform(l.id(), frame) else {
            return Ok(String::new());
        };
        count_layer(layer_count)?;
        let id = format!("{prefix}-{}", l.id());
        let mut svg = String::new();
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
                layer_text_svg(
                    text,
                    *font_size,
                    "white",
                    l.width(),
                    l.height(),
                    l.text_style()
                )
            );
            let measured =
                resvg::usvg::Tree::from_str(&source, &self.options).map_err(|e| e.to_string())?;
            let bounds = measured.root().stroke_bounding_box();
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
        let (path_defs, path_mask) = crate::path_mask_render::mask(l, &id, frame);
        svg.push_str(&path_defs);
        let a = matrix.0;
        svg.push_str(&format!(
            "<g transform='matrix({} {} {} {} {} {})' opacity='{}'>{effect_open}<g {}><g {}><g {path_mask}>",
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
            Content::Null | Content::Adjustment => {}
            Content::Rectangle | Content::Solid => svg.push_str(&format!(
                "<rect width='{}' height='{}' fill='{color}'/>",
                l.width(),
                l.height()
            )),
            Content::Shape(shape) => {
                svg.push_str(&shape.svg_at(l.width(), l.height(), l.color(), frame))
            }
            Content::Text { text, font_size } => {
                svg.push_str(&layer_text_svg(
                    text,
                    *font_size,
                    &color,
                    l.width(),
                    l.height(),
                    l.text_style(),
                ));
            }
            Content::Image { png } => {
                let png = crate::source_render::alpha_png(png, l.footage_interpretation())?;
                svg.push_str(&format!(
                    "<image width='{}' height='{}' xlink:href='data:image/png;base64,{png}'/>",
                    l.width(),
                    l.height()
                ));
            }
            Content::Composition { composition, .. } => {
                let source = project
                    .composition_by_id(*composition)
                    .ok_or("Missing source composition")?;
                if let Some(source_frame) = l.composition_frame(frame, c.fps(), source) {
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
            Content::ImageSequence { .. } => {
                if let Some(index) = l.sequence_frame(frame, c.fps()) {
                    if let Some(png) = crate::image_sequence::frame_png(
                        l.content(),
                        index,
                        l.width() as u32,
                        l.height() as u32,
                        l.footage_interpretation(),
                    )? {
                        svg.push_str(&format!("<image width='{}' height='{}' xlink:href='data:image/png;base64,{png}'/>", l.width(), l.height()));
                    }
                }
            }
            Content::Audio { .. } => {}
            Content::Video {
                path, source_fps, ..
            } => {
                if let Some(seconds) = l.video_decode_time(frame, c.fps()) {
                    let interpretation = l.footage_interpretation();
                    let alpha_changed = interpretation.alpha
                        != libre_effects_core::AlphaInterpretation::Straight
                        || interpretation.invert_alpha;
                    let png = self.decoders.lock().unwrap().frame_png(
                        path,
                        seconds,
                        *source_fps,
                        l.width() as u32,
                        l.height() as u32,
                        if alpha_changed {
                            (l.width() as u32).max(l.height() as u32)
                        } else {
                            max_dimension
                        },
                        &self.cancel,
                    )?;
                    let png = crate::source_render::alpha_png(&png, interpretation)?;
                    svg.push_str(&format!(
                        "<image width='{}' height='{}' xlink:href='data:image/png;base64,{png}'/>",
                        l.width(),
                        l.height()
                    ));
                }
            }
        }
        svg.push_str(&format!("</g></g></g>{effect_close}</g>"));

        if let Some((matte, mode)) = self.matte_pixels(
            project,
            composition,
            l,
            frame,
            max_dimension,
            &id,
            layer_count,
        )? {
            let mut source = self.raster_canvas(&svg, c.width(), c.height(), max_dimension)?;
            crate::matte_render::apply_matte(&mut source, &matte, mode);
            svg = crate::adjustment_render::embedded(&source, c.width(), c.height())?;
        }
        if svg.len() > 64 * 1024 * 1024 {
            return Err("Layer SVG exceeds 64 MiB; reduce embedded image instances".into());
        }
        Ok(svg)
    }
    pub fn render(
        &self,
        project: &Project,
        frame: u32,
        max_dimension: u32,
    ) -> Result<image::RgbaImage, String> {
        self.render_mode(project, frame, max_dimension, false, None)
    }
    pub fn render_preview(
        &self,
        project: &Project,
        frame: u32,
        max_dimension: u32,
    ) -> Result<image::RgbaImage, String> {
        self.render_mode(project, frame, max_dimension, true, None)
    }
    pub fn render_output(
        &self,
        project: &Project,
        frame: u32,
        width: u32,
        height: u32,
    ) -> Result<image::RgbaImage, String> {
        if width == 0 || height == 0 || width > 16384 || height > 16384 {
            return Err("Invalid output dimensions".into());
        }
        self.render_mode(project, frame, u32::MAX, false, Some([width, height]))
    }
    fn render_mode(
        &self,
        project: &Project,
        frame: u32,
        max_dimension: u32,
        include_guides: bool,
        output_size: Option<[u32; 2]>,
    ) -> Result<image::RgbaImage, String> {
        self.check_cancel()?;
        let c = project.composition();
        if frame >= c.duration() {
            return Err("Frame is outside the composition".into());
        }
        let scale = (max_dimension as f64 / c.width().max(c.height()) as f64).min(1.0);
        let width = (c.width() as f64 * scale).round().max(1.0) as u32;
        let height = (c.height() as f64 * scale).round().max(1.0) as u32;
        let [width, height] = output_size.unwrap_or([width, height]);
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
        self.check_cancel()?;
        let tree = resvg::usvg::Tree::from_str(&svg, &self.options).map_err(|e| e.to_string())?;
        self.check_cancel()?;
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
        self.check_cancel()?;
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
    #[test]
    fn paragraph_strokes_clip_and_join_styles_change_outline_pixels() {
        use libre_effects_core::{Command, Editor, TextStrokeJoin};
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Stroke bounds".into(),
            width: 400,
            height: 300,
            fps: 30,
            duration: 90,
        })
        .unwrap();
        let mut draft =
            crate::text_edit::Session::new_box(e.project(), 0, 0, [40.0, 30.0, 190.0, 180.0])
                .unwrap();
        draft.font_size = 120.0;
        draft.buffer.replace(None, "AV", false, None).unwrap();
        draft.style.stroke_enabled = true;
        draft.style.stroke_width = 30.0;
        draft.style.fill_enabled = false;
        e.execute(draft.command()).unwrap();
        let renderer = super::Renderer::new();
        let mut images = vec![];
        for join in [
            TextStrokeJoin::Miter,
            TextStrokeJoin::Round,
            TextStrokeJoin::Bevel,
        ] {
            let mut style = draft.style.clone();
            style.stroke_join = join;
            e.execute(Command::SetTextStyle {
                id: draft.id,
                style,
            })
            .unwrap();
            let pixels = renderer.render_output(e.project(), 0, 400, 300).unwrap();
            assert!(pixels.pixels().any(|p| p[3] > 0));
            for (x, y, p) in pixels.enumerate_pixels() {
                if !(40..230).contains(&x) || !(30..210).contains(&y) {
                    assert_eq!(p[3], 0);
                }
            }
            images.push(pixels);
        }
        assert_ne!(images[0], images[1]);
        assert_ne!(images[1], images[2]);
        let mut style = draft.style.clone();
        style.stroke_width = 0.0;
        e.execute(Command::SetTextStyle {
            id: draft.id,
            style,
        })
        .unwrap();
        assert!(
            renderer
                .render(e.project(), 0, 400)
                .unwrap()
                .pixels()
                .all(|p| p[3] == 0)
        );
    }
    #[test]
    fn text_paint_passes_preserve_layout_and_roundtrip_pixels() {
        use libre_effects_core::{Command, Editor, Project, TextStyle};
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Paint".into(),
            width: 480,
            height: 300,
            fps: 30,
            duration: 90,
        })
        .unwrap();
        let mut draft =
            crate::text_edit::Session::new(e.project(), 0, 0, None, [50.0, 50.0]).unwrap();
        draft.font_size = 64.0;
        draft
            .buffer
            .replace(None, "AVA 한글\nAV", false, None)
            .unwrap();
        draft.style.tracking = -150.0;
        draft.style.leading = 0.6;
        e.execute(draft.command()).unwrap();
        let base = e.project().clone();
        let id = e.selected().unwrap();
        let mut style = e.selected_layer().unwrap().text_style();
        let layout =
            crate::text_edit::layout::Layout::shape(&draft.buffer.text, 64.0, draft.width, &style);
        let renderer = super::Renderer::new();
        let fill = renderer.render(&base, 0, 480).unwrap();
        style.stroke_enabled = true;
        style.stroke_width = 16.0;
        style.stroke_color = 0xff0000;
        e.execute(Command::SetTextStyle {
            id,
            style: style.clone(),
        })
        .unwrap();
        let behind = renderer.render(e.project(), 0, 480).unwrap();
        assert_ne!(fill, behind);
        assert!(
            behind
                .pixels()
                .zip(fill.pixels())
                .any(|(a, b)| a[3] > 0 && b[3] == 0)
        );
        let shaped =
            crate::text_edit::layout::Layout::shape(&draft.buffer.text, 64.0, draft.width, &style);
        assert_eq!(layout.carets, shaped.carets);
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        assert_eq!(renderer.render_preview(&saved, 0, 480).unwrap(), behind);
        assert_eq!(renderer.render(&saved, 0, 480).unwrap(), behind);
        e.undo();
        assert_eq!(e.project(), &base);
        e.redo();
        assert_eq!(renderer.render(e.project(), 0, 480).unwrap(), behind);
        style.stroke_over_fill = true;
        e.execute(Command::SetTextStyle {
            id,
            style: style.clone(),
        })
        .unwrap();
        let above = renderer.render(e.project(), 0, 480).unwrap();
        assert_ne!(above, behind);
        style.fill_enabled = false;
        e.execute(Command::SetTextStyle {
            id,
            style: style.clone(),
        })
        .unwrap();
        let stroke = renderer.render(e.project(), 0, 480).unwrap();
        assert!(stroke.pixels().any(|p| p[3] > 0));
        assert!(
            stroke
                .pixels()
                .all(|p| p[3] == 0 || (p[0] > 240 && p[1] == 0 && p[2] == 0))
        );
        // Independent image compositing checks whole-layer order, including
        // overlaps between adjacent glyphs and between lines.
        for (mut reference, top, actual) in [
            (stroke.clone(), &fill, &behind),
            (fill.clone(), &stroke, &above),
        ] {
            image::imageops::overlay(&mut reference, top, 0, 0);
            assert!(
                reference.pixels().zip(actual.pixels()).all(|(a, b)| a
                    .0
                    .iter()
                    .zip(b.0)
                    .all(|(a, b)| a.abs_diff(b) <= 2))
            );
        }
        style.stroke_enabled = false;
        e.execute(Command::SetTextStyle { id, style }).unwrap();
        assert!(
            renderer
                .render(e.project(), 0, 480)
                .unwrap()
                .pixels()
                .all(|p| p[3] == 0)
        );
        // Paint changes do not change paragraph source ranges or overflow.
        let text = "한글 AVA words wrap into lines";
        let plain = TextStyle {
            paragraph: true,
            ..Default::default()
        };
        let painted = TextStyle {
            stroke_enabled: true,
            stroke_width: 40.0,
            ..plain.clone()
        };
        let a = crate::text_flow::lines(text, 48.0, 180.0, &plain);
        let b = crate::text_flow::lines(text, 48.0, 180.0, &painted);
        assert_eq!(
            a.iter().map(|l| (&l.range, l.bottom)).collect::<Vec<_>>(),
            b.iter().map(|l| (&l.range, l.bottom)).collect::<Vec<_>>()
        );
    }
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
    fn independent_temporal_handles_match_preview_output_and_saved_pixels() {
        use libre_effects_core::TemporalHandle;
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Velocity".into(),
            width: 200,
            height: 120,
            fps: 30,
            duration: 90,
        })
        .unwrap();
        e.execute(Command::AddContent {
            content: Content::Rectangle,
            width: 20.0,
            height: 20.0,
            name: "Moving".into(),
        })
        .unwrap();
        for (frame, value) in [(0, 40.0), (30, 100.0), (60, 40.0)] {
            e.execute(Command::ToggleKeyframe {
                id: 1,
                property: Property::PositionX,
                frame,
            })
            .unwrap();
            e.execute(Command::SetValue {
                id: 1,
                property: Property::PositionX,
                frame,
                value,
            })
            .unwrap();
        }
        let original = e.project().clone();
        for (incoming, slope, influence) in [(true, 0.0, 0.7), (false, -5.0, 1.0 / 3.0)] {
            e.execute(Command::SetTemporalHandle {
                id: 1,
                property: Property::PositionX.into(),
                frame: 30,
                incoming,
                handle: TemporalHandle { slope, influence },
            })
            .unwrap();
        }
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        let renderer = Renderer::new();
        for frame in [0, 15, 30, 45, 60] {
            let output = renderer.render_output(&saved, frame, 200, 120).unwrap();
            assert_eq!(output, renderer.render(e.project(), frame, 200).unwrap());
            if frame == 15 || frame == 45 {
                assert_ne!(output, renderer.render(&original, frame, 200).unwrap());
            } else {
                assert_eq!(output, renderer.render(&original, frame, 200).unwrap());
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
    fn fractional_rate_and_timecode_preserve_keyframe_pixels_and_nested_samples() {
        let mut e = scene();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::ToggleKeyframe {
            id: 1,
            property: Property::Opacity,
            frame: 0,
        })
        .unwrap();
        e.execute(Command::SetValue {
            id: 1,
            property: Property::Opacity,
            frame: 8,
            value: 0.0,
        })
        .unwrap();
        let renderer = Renderer::new();
        let before = renderer.render(e.project(), 4, 100).unwrap();
        e.execute(Command::ConfigureCompositionRate {
            name: "Film".into(),
            width: 100,
            height: 100,
            fps: "23.976".parse().unwrap(),
            duration: 10,
            display_start: 86400,
        })
        .unwrap();
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        assert_eq!(renderer.render(&saved, 4, 100).unwrap(), before);
        assert_eq!(renderer.render_preview(&saved, 4, 100).unwrap(), before);
        e.execute(Command::NewComposition).unwrap();
        e.execute(Command::ConfigureCompositionRate {
            name: "NTSC".into(),
            width: 100,
            height: 100,
            fps: "29.97".parse().unwrap(),
            duration: 20,
            display_start: 0,
        })
        .unwrap();
        e.execute(Command::AddCompositionLayer {
            composition: 1,
            frame: 0,
        })
        .unwrap();
        assert_eq!(renderer.render(e.project(), 5, 100).unwrap(), before);
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
    fn shape_fill_stroke_and_typography_use_the_shared_compositor() {
        use libre_effects_core::{Shape, ShapeKind, TextAlign, TextStyle};
        let mut e = scene();
        e.execute(Command::AddContent {
            content: Content::Shape(Shape {
                kind: ShapeKind::Ellipse,
                ..Default::default()
            }),
            width: 60.0,
            height: 60.0,
            name: "Ellipse".into(),
        })
        .unwrap();
        let id = e.selected().unwrap();
        e.execute(Command::SetColor {
            id,
            color: 0xff3300,
        })
        .unwrap();
        let r = Renderer::new();
        let filled = r.render(e.project(), 0, 100).unwrap();
        assert_eq!(filled.get_pixel(50, 50).0, [255, 51, 0, 255]);
        assert_eq!(filled.get_pixel(21, 21)[3], 0);
        e.execute(Command::SetContent {
            id,
            content: Content::Shape(Shape {
                kind: ShapeKind::Ellipse,
                fill: false,
                stroke_color: 0x00ff00,
                stroke_width: 6.0,
                ..Default::default()
            }),
        })
        .unwrap();
        let stroked = r.render(e.project(), 0, 100).unwrap();
        assert_eq!(stroked.get_pixel(50, 50)[3], 0);
        assert!(stroked.pixels().any(|p| p[1] > 200 && p[3] > 200));
        e.execute(Command::SetContent {
            id,
            content: Content::Text {
                text: "AA\nBB".into(),
                font_size: 12.0,
            },
        })
        .unwrap();
        let left = r.render(e.project(), 0, 100).unwrap();
        e.execute(Command::SetTextStyle {
            id,
            style: TextStyle {
                leading: 2.0,
                tracking: 150.0,
                align: TextAlign::Right,
                ..Default::default()
            },
        })
        .unwrap();
        let right = r.render(e.project(), 0, 100).unwrap();
        assert_ne!(left, right);
        let bounds = |img: &image::RgbaImage| {
            img.enumerate_pixels()
                .filter(|(_, _, p)| p[3] > 50)
                .fold((u32::MAX, 0, 0), |(min, max, bottom), (x, y, _)| {
                    (min.min(x), max.max(x), bottom.max(y))
                })
        };
        assert!(bounds(&right).0 > bounds(&left).0);
        assert!(bounds(&right).2 > bounds(&left).2);
        let restored = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        assert_eq!(r.render(&restored, 0, 100).unwrap(), right);
        assert_eq!(r.render_preview(&restored, 0, 100).unwrap(), right);
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
