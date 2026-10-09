//! One compositing path for preview, stills and frame sequences.
use base64::{Engine, engine::general_purpose::STANDARD};
#[cfg(test)]
use libre_effects_core::Property;
use libre_effects_core::{
    CompositionSample, CompositionSampleKey, Content, Project, TextPaint, TextParam,
};
use std::{fmt, io::Cursor, path::Path, sync::Arc};

/// The ceiling includes wrappers and copies, not only Contents' path payload.
pub(crate) const SVG_LIMIT: usize = 64 * 1024 * 1024;

/// Count formatting before reserving or copying so a rejected nested paint/image
/// never allocates an unbounded intermediate `format!` string.
pub(crate) fn append_svg(output: &mut String, args: fmt::Arguments<'_>) -> Result<(), String> {
    struct Count(usize);
    impl fmt::Write for Count {
        fn write_str(&mut self, value: &str) -> fmt::Result {
            self.0 = self.0.checked_add(value.len()).ok_or(fmt::Error)?;
            if self.0 > SVG_LIMIT {
                return Err(fmt::Error);
            }
            Ok(())
        }
    }
    if output.len() > SVG_LIMIT {
        return Err("Frame SVG exceeds 64 MiB; simplify the composition".into());
    }
    let mut count = Count(output.len());
    fmt::write(&mut count, args)
        .map_err(|_| "Frame SVG exceeds 64 MiB; simplify the composition".to_string())?;
    output
        .try_reserve_exact(count.0 - output.len())
        .map_err(|_| "Could not allocate bounded frame SVG".to_string())?;
    fmt::write(output, args).map_err(|_| "Could not format frame SVG".to_string())
}

pub(crate) fn svg_document(body: &str, width: f64, height: f64) -> Result<String, String> {
    let mut output = String::new();
    append_svg(
        &mut output,
        format_args!(
            "<svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink' width='{width}' height='{height}'>{body}</svg>"
        ),
    )?;
    Ok(output)
}

/// One value is shared by every nested composition and matte in a frame.
#[derive(Default)]
pub(crate) struct FrameRenderBudget {
    layer_instances: usize,
    audio_analysis: crate::audio_analysis::FrameAnalysis,
    pub(crate) repeat_domains: Vec<resvg::RepeatEdgeDomain>,
    contents: libre_effects_core::ContentsRenderBudget,
    expression_source: Option<Arc<Project>>,
    expression_values: std::collections::BTreeMap<
        (u64, CompositionSampleKey, bool),
        Arc<libre_effects_core::expression_runtime::EvaluatedProperties>,
    >,
    expression_modes: std::collections::BTreeMap<(u64, CompositionSampleKey), bool>,
    expression_root: Option<(u64, CompositionSampleKey, bool)>,
    sampled_views: std::collections::BTreeMap<(u64, CompositionSampleKey, bool), Arc<Project>>,
    sample_times: std::collections::BTreeMap<u64, CompositionSample>,
    expression_view: Option<Arc<Project>>,
}

impl FrameRenderBudget {
    pub(crate) fn register_repeat_domains(
        &mut self,
        domains: Vec<resvg::RepeatEdgeDomain>,
    ) -> Result<(), String> {
        for domain in domains {
            if let Some(previous) = self.repeat_domains.iter().find(|previous| {
                previous.filter_id == domain.filter_id
                    && previous.primitive_index == domain.primitive_index
            }) {
                if previous.rect != domain.rect || previous.transform != domain.transform {
                    return Err("Conflicting Repeat Edge Pixels source domains".into());
                }
                continue;
            }
            if self.repeat_domains.len() >= 4096 {
                return Err("A frame exceeds 4096 Repeat Edge Pixels domains".into());
            }
            self.repeat_domains
                .try_reserve(1)
                .map_err(|_| "Could not allocate repeat-edge metadata")?;
            self.repeat_domains.push(domain);
        }
        Ok(())
    }
}

pub(crate) fn frame_pixmap(
    width: u32,
    height: u32,
    checked: bool,
) -> Result<resvg::tiny_skia::Pixmap, String> {
    if !checked {
        return resvg::tiny_skia::Pixmap::new(width, height)
            .ok_or_else(|| "Could not allocate render buffer".into());
    }
    let count = u64::from(width)
        .checked_mul(u64::from(height))
        .filter(|n| *n > 0 && *n <= 33_554_432)
        .ok_or("Checked render exceeds 32 megapixels")?;
    let bytes = usize::try_from(count.checked_mul(4).ok_or("Render allocation overflow")?)
        .map_err(|_| "Render allocation overflow")?;
    let mut data = Vec::new();
    data.try_reserve_exact(bytes)
        .map_err(|_| "Could not allocate checked render buffer")?;
    data.resize(bytes, 0);
    let size =
        resvg::tiny_skia::IntSize::from_wh(width, height).ok_or("Invalid render dimensions")?;
    resvg::tiny_skia::Pixmap::from_vec(data, size)
        .ok_or_else(|| "Invalid checked render buffer".into())
}

pub(crate) fn paint_svg_tree(
    tree: &resvg::usvg::Tree,
    transform: resvg::tiny_skia::Transform,
    pixels: &mut resvg::tiny_skia::PixmapMut<'_>,
    domains: &[resvg::RepeatEdgeDomain],
) -> Result<(), String> {
    let box3 = tree.filters().iter().any(|filter| filter.primitives().iter().any(|primitive| matches!(primitive.kind(), resvg::usvg::filter::Kind::GaussianBlur(blur) if blur.box3_radius().is_some())));
    let byte257 = tree.has_opaque_opacity_byte257();
    if domains.is_empty() && !box3 && !byte257 {
        resvg::render(tree, transform, pixels);
        return Ok(());
    }
    // A measured source-project mixed mask needs a 6875x5077 Gaussian support
    // buffer (34,904,375 pixels). Keep the output's 32-MP limit independent
    // from bounded temporary support; ordinary opacity-only output is unchanged.
    let mixed_masks = byte257 && box3 && domains.is_empty();
    resvg::render_checked(
        tree,
        transform,
        pixels,
        &resvg::CheckedRenderOptions {
            repeat_edge_domains: domains,
            limits: resvg::RenderLimits {
                max_pixels: if mixed_masks { 67_108_864 } else { 33_554_432 },
                max_bytes: if mixed_masks {
                    256 * 1024 * 1024
                } else {
                    128 * 1024 * 1024
                },
                // Profiled compositions may combine Box3 masks with large
                // ordinary Gaussian groups. Full support and retained filter
                // buffers share this cap, including Gaussian scratch copies.
                max_live_bytes: if mixed_masks {
                    1024 * 1024 * 1024
                } else if byte257 && domains.is_empty() {
                    512 * 1024 * 1024
                } else {
                    256 * 1024 * 1024
                },
            },
        },
    )
    .map_err(|error| {
        if byte257 {
            format!("Opacity compositing: {error}")
        } else if box3 {
            format!("Mask Feather: {error}")
        } else {
            format!("Repeat Edge Pixels: {error}")
        }
    })
}

pub(crate) struct RenderedFrame {
    pub pixels: image::RgbaImage,
    /// Exact, nonserializable geometry used for the root composition's pixels.
    pub evaluated: Option<Arc<Project>>,
}

#[cfg(test)]
thread_local! {
    static TEST_CONTENTS_BUDGET: std::cell::RefCell<Option<libre_effects_core::ContentsRenderBudget>> = const { std::cell::RefCell::new(None) };
}
/// Capture the lowered limits at Renderer construction; the resulting Renderer
/// keeps them even when a preview task moves to another thread. Restore on panic.
#[cfg(test)]
pub(crate) fn with_test_contents_budget<T>(
    budget: libre_effects_core::ContentsRenderBudget,
    run: impl FnOnce() -> T,
) -> T {
    struct Restore(Option<libre_effects_core::ContentsRenderBudget>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TEST_CONTENTS_BUDGET.with(|budget| *budget.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(TEST_CONTENTS_BUDGET.with(|current| current.replace(Some(budget))));
    run()
}

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
    audio_analysis: std::sync::Mutex<crate::audio_analysis::AudioAnalysis>,
    expression_worker: std::sync::Mutex<crate::automation_process::ExpressionSession>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    image_sources: Arc<std::sync::Mutex<crate::render_images::Sources>>,
    raster_images: Arc<std::sync::Mutex<resvg::RasterImageCache>>,
    pub(crate) raster_scenes: std::sync::Mutex<crate::raster_scenes::Cache>,
    filter_cache: Arc<std::sync::Mutex<resvg::FilterCache>>,
    text_compositions: Arc<std::sync::Mutex<crate::rich_text_render::CompositionCache>>,
    paths: Arc<std::sync::Mutex<resvg::usvg::PathCache>>,
    #[cfg(test)]
    contents_budget: libre_effects_core::ContentsRenderBudget,
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
    libre_effects_core::text_paragraphs::paragraphs(text).enumerate().map(|(line,paragraph)| format!("<text x='{x}' y='{}' text-anchor='{anchor}' letter-spacing='{}' font-family='{}' font-weight='{}' font-style='{}' font-size='{font_size}' fill='{color}' xml:space='preserve'>{}</text>",font_size * (1.0 + style.leading * line as f64),style.tracking * font_size / 1000.0,xml(crate::fonts::svg_family(&style)),style.weight, if style.italic { "italic" } else { "normal" },xml(paragraph.text))).collect()
}
/// Compose the same complete fill and stroke passes used at full opacity. The
/// optional second SVG retains their unattenuated geometry for effect bounds.
#[cfg(test)]
fn layer_text_svg(
    text: &str,
    size: f64,
    color: &str,
    width: f64,
    height: f64,
    style: libre_effects_core::TextStyle,
    opacity: [f64; 2],
    retain_bounds: bool,
    animator: libre_effects_core::TextAnimatorSample,
    prefix: &str,
) -> Result<(String, Option<String>), String> {
    layer_text_animators_svg(
        text,
        size,
        color,
        width,
        height,
        style,
        opacity,
        retain_bounds,
        &[animator],
        prefix,
    )
}

fn layer_text_animators_svg(
    text: &str,
    size: f64,
    color: &str,
    width: f64,
    height: f64,
    style: libre_effects_core::TextStyle,
    opacity: [f64; 2],
    retain_bounds: bool,
    animators: &[libre_effects_core::TextAnimatorSample],
    prefix: &str,
) -> Result<(String, Option<String>), String> {
    let active: Vec<_> = animators
        .iter()
        .filter(|animator| !animator.is_identity())
        .map(|animator| {
            (
                crate::text_animator::Selection::new(text, animator),
                animator,
            )
        })
        .filter(|(selection, _)| !selection.is_empty())
        .collect();
    let geometry = |color: &str| {
        if !active.is_empty() {
            crate::text_animator_render::source_geometry_svg(
                text, size, color, width, height, &style,
            )
        } else {
            Ok(text_geometry_svg(text, size, color, width, height, &style))
        }
    };
    let fill = if style.fill_enabled {
        geometry(color)?
    } else {
        String::new()
    };
    let stroke = if style.stroke_enabled && style.stroke_width != 0.0 {
        let join = match style.stroke_join {
            libre_effects_core::TextStrokeJoin::Miter => "miter",
            libre_effects_core::TextStrokeJoin::Round => "round",
            libre_effects_core::TextStrokeJoin::Bevel => "bevel",
        };
        format!(
            "<g stroke='#{:06x}' stroke-width='{}' stroke-linejoin='{join}' stroke-miterlimit='4'>{}</g>",
            style.stroke_color,
            style.stroke_width,
            geometry("none")?
        )
    } else {
        String::new()
    };
    let animate = |pass: String, paint: &str| {
        let prefix = format!("{prefix}-animator-{paint}-");
        match active.as_slice() {
            [] => Ok((pass, None)),
            // Identity extras and empty selectors must not change the established
            // single-animator geometry, float association or paint aggregation.
            [(selection, sample)] => crate::text_animator_render::animate_pass(
                &pass,
                text,
                selection,
                sample,
                width,
                height,
                &prefix,
                retain_bounds,
            ),
            _ => crate::text_animator_render::animate_stack_pass(
                &pass,
                text,
                &active,
                width,
                height,
                &prefix,
                retain_bounds,
            ),
        }
    };
    let (fill, fill_bounds) = animate(fill, "fill")?;
    let (stroke, stroke_bounds) = animate(stroke, "stroke")?;
    // Retain translated geometry before either protected-unit or pass opacity.
    let attenuated = (!fill.is_empty() && opacity[0] != 100.0)
        || (!stroke.is_empty() && opacity[1] != 100.0)
        || fill_bounds.is_some()
        || stroke_bounds.is_some();
    let bounds = (retain_bounds && attenuated).then(|| {
        let fill = fill_bounds.as_deref().unwrap_or(&fill);
        let stroke = stroke_bounds.as_deref().unwrap_or(&stroke);
        if style.stroke_over_fill {
            format!("{fill}{stroke}")
        } else {
            format!("{stroke}{fill}")
        }
    });
    let fill = text_pass_opacity(fill, opacity[0]);
    let stroke = text_pass_opacity(stroke, opacity[1]);
    let paint = if stroke.is_empty() {
        fill
    } else if style.stroke_over_fill {
        format!("{fill}{stroke}")
    } else {
        format!("{stroke}{fill}")
    };
    Ok((paint, bounds))
}

fn text_pass_opacity(pass: String, percent: f64) -> String {
    if pass.is_empty() || percent == 100.0 {
        // A full-opacity group would add no visual intent. Leave the old pass
        // unchanged, including its existing rasterization and rounding stages.
        pass
    } else {
        // Group opacity attenuates the complete pass exactly once, even where
        // its glyphs/lines overlap. Do not quantize this float to a byte matte.
        // Retain zero-opacity geometry, including paragraph clipping.
        format!("<g opacity='{}'>{pass}</g>", percent / 100.0)
    }
}
// Keep paint out of shaping and caret metrics. Separate whole-layer passes also
// preserve the chosen ordering when characters or lines overlap.
pub(crate) fn text_geometry_svg(
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
    for line in lines
        .iter()
        .take(crate::text_flow::composed_count(&lines, height))
    {
        let (x, y) = (line.x, line.y);
        svg.push_str(&format!(
            "<g transform='translate({x} {y})'>{}</g>",
            text_svg(
                &text[line.range.start..line.visible_end],
                size,
                color,
                line.width,
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
fn count_layer(budget: &mut FrameRenderBudget) -> Result<(), String> {
    budget.layer_instances = budget
        .layer_instances
        .checked_add(1)
        .ok_or("Frame layer instance count overflow")?;
    if budget.layer_instances > 4096 {
        return Err(
            "Frame exceeds 4096 nested layer or matte instances; simplify the composition".into(),
        );
    }
    Ok(())
}

impl Renderer {
    pub(crate) fn filter_cache_hits(&self) -> u64 {
        self.filter_cache.lock().unwrap().hits()
    }
    pub(crate) fn decoder_status(&self) -> crate::video_decoder::Status {
        self.decoders.lock().unwrap().status()
    }
    pub fn new() -> Self {
        Self::with_cancel(Default::default())
    }
    pub fn with_cancel(cancel: Arc<std::sync::atomic::AtomicBool>) -> Self {
        let mut options = crate::fonts::render_options();
        options.image_href_resolver.resolve_string = crate::render_images::resolver();
        Self {
            options,
            decoders: Default::default(),
            audio_analysis: Default::default(),
            cancel,
            expression_worker: Default::default(),
            image_sources: Default::default(),
            raster_scenes: Default::default(),
            filter_cache: Arc::new(std::sync::Mutex::new(resvg::FilterCache::new(
                128 * 1024 * 1024,
            ))),
            text_compositions: Default::default(),
            paths: Arc::new(std::sync::Mutex::new(resvg::usvg::PathCache::new(
                16 * 1024 * 1024,
            ))),
            raster_images: Arc::new(std::sync::Mutex::new(resvg::RasterImageCache::new(
                128 * 1024 * 1024,
            ))),
            #[cfg(test)]
            contents_budget: TEST_CONTENTS_BUDGET
                .with(|budget| budget.borrow().clone().unwrap_or_default()),
        }
    }
    fn frame_budget(&self) -> FrameRenderBudget {
        FrameRenderBudget {
            layer_instances: 0,
            audio_analysis: Default::default(),
            repeat_domains: Vec::new(),
            expression_source: None,
            expression_values: Default::default(),
            expression_modes: Default::default(),
            expression_root: None,
            sampled_views: Default::default(),
            sample_times: Default::default(),
            expression_view: None,
            #[cfg(test)]
            contents: self.contents_budget.clone(),
            #[cfg(not(test))]
            contents: Default::default(),
        }
    }
    pub fn clear_decoders(&self) {
        self.decoders.lock().unwrap().clear();
        *self.audio_analysis.lock().unwrap() = Default::default();
        self.expression_worker.lock().unwrap().clear();
        self.image_sources.lock().unwrap().clear();
        self.raster_images.lock().unwrap().clear();
        self.raster_scenes.lock().unwrap().clear();
        self.filter_cache.lock().unwrap().clear();
        self.text_compositions.lock().unwrap().clear();
        self.paths.lock().unwrap().clear();
    }
    pub(crate) fn path_cache_hits(&self) -> u64 {
        self.paths.lock().map_or(0, |s| s.hits())
    }
    pub(crate) fn check_cancel(&self) -> Result<(), String> {
        crate::video_decoder::check_cancel(&self.cancel)
    }
    fn expression_view(
        &self,
        project: &Project,
        composition: u64,
        frame: u32,
        include_guides: bool,
        budget: &mut FrameRenderBudget,
    ) -> Result<Option<Arc<Project>>, String> {
        let _time = crate::gpu_render::time(crate::gpu_render::Stage::Expression);
        let sample = budget
            .sample_times
            .get(&composition)
            .copied()
            .map(Ok)
            .unwrap_or_else(|| {
                let comp = project
                    .composition_by_id(composition)
                    .ok_or("Missing source composition")?;
                CompositionSample::from_frame(frame, comp.fps())
            })?;
        if project.evaluated_at_sample(composition, sample) {
            return Ok(None);
        }
        // Never take the next child snapshot from an already-frozen ancestor.
        let (source, roots) = if let Some(source) = budget.expression_source.clone() {
            let roots = source.expression_roots_at_sample(composition, sample, include_guides)?;
            if roots.is_empty() && !sample.is_fractional() {
                return Ok(None);
            }
            (source, roots)
        } else {
            if project.evaluated_frame().is_some() {
                return Err("Evaluated scene lost its authored source".into());
            }
            let roots = project.expression_roots_at_sample(composition, sample, include_guides)?;
            if roots.is_empty() && !sample.is_fractional() {
                return Ok(None);
            }
            let source = Arc::new(project.clone());
            budget.expression_source = Some(source.clone());
            (source, roots)
        };
        let key = (composition, sample.key(), include_guides);
        if let Some(view) = budget.sampled_views.get(&key) {
            return Ok(Some(view.clone()));
        }
        if budget.sampled_views.len() >= 128 {
            return Err("A frame exceeds 128 sampled composition/time views".into());
        }
        let values = if roots.is_empty() {
            // Pure authored fractional sampling still needs a detached view.
            Arc::new(
                libre_effects_core::expression_runtime::EvaluatedProperties {
                    composition: libre_effects_core::expression_runtime::CompositionId(composition),
                    time: sample.seconds(),
                    values: Default::default(),
                    dependencies: Default::default(),
                    expression_evaluations: 0,
                    host_reads: 0,
                },
            )
        } else if let Some(values) = budget.expression_values.get(&key) {
            values.clone()
        } else {
            if budget.expression_values.len() >= 128 {
                return Err("A frame exceeds 128 expression composition/time evaluations".into());
            }
            let snapshot = source.expression_snapshot_at_sample(composition, sample)?;
            let values = self
                .expression_worker
                .lock()
                .map_err(|_| "Expression worker is unavailable")?
                .evaluate(
                    &snapshot,
                    &roots,
                    &self.cancel,
                    std::time::Duration::from_secs(2),
                )
                .map_err(|error| {
                    let property = error
                        .property
                        .as_ref()
                        .map(|property| {
                            let layer = source
                                .composition_by_id(property.composition.0)
                                .and_then(|comp| comp.layer(property.layer.0));
                            format!(
                                " · layer '{}' ({}) {:?}",
                                layer.map_or("unknown", |layer| layer.name()),
                                property.layer.0,
                                property.property
                            )
                        })
                        .unwrap_or_default();
                    format!(
                        "Expression {:?} in composition {composition}{property}: {}",
                        error.kind, error.message
                    )
                })?;
            let values = Arc::new(values);
            budget.expression_values.insert(key, values.clone());
            values
        };
        let view = Arc::new(source.with_evaluated_properties_at_sample(
            composition,
            sample,
            include_guides,
            &values,
        )?);
        if budget.expression_root == Some(key) {
            budget.expression_view = Some(view.clone());
        }
        budget.sampled_views.insert(key, view.clone());
        Ok(Some(view))
    }
    fn layers_svg(
        &self,
        project: &Project,
        composition: libre_effects_core::CompositionId,
        frame: u32,
        max_dimension: u32,
        prefix: &str,
        budget: &mut FrameRenderBudget,
        include_guides: bool,
    ) -> Result<String, String> {
        let comp = project
            .composition_by_id(composition)
            .ok_or("Missing source composition")?;
        self.layers_svg_at_sample(
            project,
            composition,
            CompositionSample::from_frame(frame, comp.fps())?,
            max_dimension,
            prefix,
            budget,
            include_guides,
        )
    }
    fn layers_svg_at_sample(
        &self,
        project: &Project,
        composition: libre_effects_core::CompositionId,
        sample: CompositionSample,
        max_dimension: u32,
        prefix: &str,
        budget: &mut FrameRenderBudget,
        include_guides: bool,
    ) -> Result<String, String> {
        let comp = project
            .composition_by_id(composition)
            .ok_or("Missing source composition")?;
        let sample = sample.in_rate(comp.fps())?;
        let frame = sample.floor_frame()?;
        let previous = budget.sample_times.insert(composition, sample);
        let result = self.layers_svg_sampled(
            project,
            composition,
            frame,
            max_dimension,
            prefix,
            budget,
            include_guides,
        );
        if let Some(previous) = previous {
            budget.sample_times.insert(composition, previous);
        } else {
            budget.sample_times.remove(&composition);
        }
        result
    }
    fn layers_svg_sampled(
        &self,
        project: &Project,
        composition: libre_effects_core::CompositionId,
        frame: u32,
        max_dimension: u32,
        prefix: &str,
        budget: &mut FrameRenderBudget,
        include_guides: bool,
    ) -> Result<String, String> {
        let sample = *budget
            .sample_times
            .get(&composition)
            .ok_or("Missing render sample context")?;
        budget
            .expression_modes
            .insert((composition, sample.key()), include_guides);
        let evaluated =
            self.expression_view(project, composition, frame, include_guides, budget)?;
        let project = evaluated.as_deref().unwrap_or(project);
        let c = project
            .composition_by_id(composition)
            .ok_or("Missing source composition")?;
        let mut svg = String::new();
        // Projection and ordering are shared with preview geometry. Unsupported
        // spatial scenes fail before painting instead of becoming empty pixels.
        for id in c.render_order(frame, include_guides)? {
            let l = c
                .layer(id)
                .ok_or("Render order references a missing layer")?;
            if matches!(l.content(), Content::Null | Content::Audio { .. }) {
                continue;
            }
            self.check_cancel()?;
            if matches!(l.content(), Content::Adjustment) {
                let matrix = c.projected_geometry(l.id(), frame)?.transform;
                count_layer(budget)?;
                let id = format!("{prefix}-{}", l.id());
                let matte =
                    self.matte_pixels(project, composition, l, frame, max_dimension, &id, budget)?;
                svg = self.adjust_composite(
                    &svg,
                    l,
                    frame,
                    c.fps().seconds(1),
                    matrix,
                    c.width(),
                    c.height(),
                    max_dimension,
                    &id,
                    matte.as_ref().map(|(p, m)| (p, *m)),
                    budget,
                )?;
            } else {
                let source = self.isolated_layer_svg(
                    project,
                    composition,
                    l.id(),
                    frame,
                    max_dimension,
                    prefix,
                    budget,
                )?;
                if l.blend_mode() == libre_effects_core::BlendMode::Normal {
                    append_svg(&mut svg, format_args!("{source}"))?;
                } else {
                    svg = self.blend_composite(
                        &svg,
                        &source,
                        l.blend_mode(),
                        c.width(),
                        c.height(),
                        max_dimension,
                        &budget.repeat_domains,
                    )?;
                }
            }
            if svg.len() > SVG_LIMIT {
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
        budget: &mut FrameRenderBudget,
    ) -> Result<String, String> {
        let incoming_project = project;
        let sample = budget
            .sample_times
            .get(&composition)
            .copied()
            .map(Ok)
            .unwrap_or_else(|| {
                let comp = project
                    .composition_by_id(composition)
                    .ok_or("Missing source composition")?;
                CompositionSample::from_frame(frame, comp.fps())
            })?;
        let include_guides = budget
            .expression_modes
            .get(&(composition, sample.key()))
            .copied()
            .unwrap_or(false);
        let evaluated =
            self.expression_view(project, composition, frame, include_guides, budget)?;
        let project = evaluated.as_deref().unwrap_or(project);
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
        let matrix = c.projected_geometry(l.id(), frame)?.transform;
        count_layer(budget)?;
        let id = format!("{prefix}-{}", l.id());
        let mut svg = String::new();
        let e = l.effects();
        let generated_spectrum = if let Some(effect) = l.effect_stack().iter().find(|effect| {
            !effect.bypassed() && effect.kind() == libre_effects_core::EffectKind::AudioSpectrum
        }) {
            if !matches!(l.content(), Content::Rectangle | Content::Solid) || l.is_three_d() {
                return Err("Audio Spectrum requires a 2D Rectangle or Solid".into());
            }
            let settings = effect
                .audio_spectrum()
                .ok_or("Missing Audio Spectrum settings")?;
            let source_snapshot = budget.expression_source.clone();
            let authored = source_snapshot.as_deref().unwrap_or(incoming_project);
            let context = |error: String| {
                format!(
                    "Audio Spectrum in composition '{}' ({composition}), layer '{}' ({}), effect '{}' ({}) at {:.9}s, source {:?}: {error}",
                    c.name(),
                    l.name(),
                    l.id(),
                    effect.name(),
                    effect.id(),
                    sample.seconds(),
                    settings.source.map(|source| source.layer),
                )
            };
            let frame = self
                .audio_analysis
                .lock()
                .map_err(|_| "Audio analysis worker state is unavailable".to_string())?
                .analyze(
                    authored,
                    composition,
                    sample,
                    settings,
                    &mut budget.audio_analysis,
                    &self.cancel,
                )
                .map_err(context)?;
            let paint = match frame {
                Some(frame) => crate::audio_spectrum_render::spectrum_svg(
                    settings,
                    &frame.amplitudes,
                    l.width(),
                    l.height(),
                    &format!("{id}-spectrum"),
                    &self.cancel,
                )
                .map_err(context)?,
                None => String::new(),
            };
            Some((effect.id(), settings.composite_original, paint))
        } else {
            None
        };
        // Sample geometry and paint once for both measurement and painting.
        // Keep the source layer untouched, including during in-between frames.
        let text_paint = matches!(l.content(), Content::Text { .. }).then(|| {
            let mut style = l.text_style();
            let typography = l.text_typography_at(frame).unwrap();
            typography.apply_to_style(&mut style);
            style.stroke_color = l.text_color_at(TextPaint::Stroke, frame).unwrap();
            style.stroke_width = l.text_value_at(TextParam::StrokeWidth, frame).unwrap();
            let fill = format!("#{:06x}", l.text_color_at(TextPaint::Fill, frame).unwrap());
            let opacity = [TextPaint::Fill, TextPaint::Stroke]
                .map(|paint| l.text_value_at(paint.opacity(), frame).unwrap());
            (fill, style, typography.font_size, opacity)
        });
        let measure_effect_bounds = matches!(
            l.content(),
            Content::Text { .. } | Content::ShapeContents(_)
        ) && (!l.path_masks().is_empty()
            || l.effect_stack().iter().any(|e| {
                !e.bypassed() && e.kind() != libre_effects_core::EffectKind::SliderControl
            }));
        let mut text_bounds_svg = None;
        let sampled_svg = match l.content() {
            Content::ShapeContents(contents) => Some(
                contents
                    .svg_at_with_budget(
                        frame,
                        &id,
                        &mut budget.contents,
                        Some(&|| self.cancel.load(std::sync::atomic::Ordering::Relaxed)),
                    )
                    .map_err(|error| {
                        format!(
                            "Composition '{}' ({composition}), layer '{}' ({}): {error}",
                            c.name(),
                            l.name(),
                            l.id(),
                        )
                    })?,
            ),
            Content::Text { .. } => {
                let text = l.source_text_at(frame).unwrap();
                let (fill, style, font_size, opacity) = text_paint.as_ref().unwrap();
                let (paint, bounds) = if let Some(rich) = l.rich_text() {
                    crate::rich_text_render::layer_svg(
                        text,
                        rich,
                        l.width(),
                        style,
                        *opacity,
                        measure_effect_bounds,
                    )
                    .map_err(|error| format!("Layer '{}' ({}): {error}", l.name(), l.id()))?
                } else {
                    // Keep the exact established path for every legacy layer.
                    layer_text_animators_svg(
                        text,
                        *font_size,
                        fill,
                        l.width(),
                        l.height(),
                        style.clone(),
                        *opacity,
                        measure_effect_bounds,
                        &l.text_animators_at(frame).unwrap(),
                        &id,
                    )?
                };
                text_bounds_svg = bounds;
                Some(paint)
            }
            _ => None,
        };
        let mut effect_bounds = [0.0, 0.0, l.width(), l.height()];
        if measure_effect_bounds {
            // Point text can extend outside the layer's nominal size. Measure
            // the same enabled/evaluated glyph geometry before paint opacity so
            // transparent passes cannot shrink or shift the filter allocation.
            let geometry = text_bounds_svg
                .as_deref()
                .or(sampled_svg.as_deref())
                .unwrap();
            let source = svg_document(geometry, l.width(), l.height())?;
            let measured =
                resvg::usvg::Tree::from_str(&source, &self.options).map_err(|e| e.to_string())?;
            let bounds = measured.root().stroke_bounding_box();
            let mut left = f64::from(bounds.left()).min(0.0);
            let mut top = f64::from(bounds.top()).min(0.0);
            let mut right = f64::from(bounds.right()).max(l.width());
            let mut bottom = f64::from(bounds.bottom()).max(l.height());
            if text_paint.is_some() {
                // The raster backend floors a filter's origin and ceils its
                // size independently. Outward-round text extents first so a
                // fractional stroked edge is not dropped by that conversion.
                left = left.floor();
                top = top.floor();
                right = right.ceil();
                bottom = bottom.ceil();
            }
            effect_bounds = [left, top, right - left, bottom - top];
        }
        let domain = crate::effect_render::InputDomain::local(effect_bounds);
        let stack = if let Some((effect, _, _)) = &generated_spectrum {
            crate::effect_render::stack_with_generated_spectrum(
                l,
                frame,
                &id,
                effect_bounds,
                domain,
                *effect,
            )?
        } else {
            crate::effect_render::stack_with_domain(l, frame, &id, effect_bounds, domain)?
        };
        budget.register_repeat_domains(stack.repeat_domains)?;
        let (effect_defs, effect_open, effect_close) = (stack.definitions, stack.open, stack.close);
        append_svg(&mut svg, format_args!("{effect_defs}"))?;
        append_svg(
            &mut svg,
            format_args!(
                "<defs><filter id='fx{id}' x='-100%' y='-100%' width='300%' height='300%'>"
            ),
        )?;
        if e.blur > 0.0 {
            append_svg(
                &mut svg,
                format_args!("<feGaussianBlur stdDeviation='{}'/>", e.blur),
            )?;
        }
        if e.grayscale {
            append_svg(
                &mut svg,
                format_args!("<feColorMatrix type='saturate' values='0'/>"),
            )?;
        }
        if e.brightness != 1.0 {
            let b = e.brightness;
            append_svg(
                &mut svg,
                format_args!(
                    "<feColorMatrix values='{b} 0 0 0 0 0 {b} 0 0 0 0 0 {b} 0 0 0 0 0 1 0'/>"
                ),
            )?;
        }
        append_svg(&mut svg, format_args!("</filter>"))?;
        if let Some(m) = l.mask() {
            append_svg(
                &mut svg,
                format_args!(
                    "<clipPath id='mask{id}'><path clip-rule='evenodd' d='{}M{} {}h{}v{}h{}z'/></clipPath>",
                    if m.inverted {
                        format!("M0 0h{}v{}h{}z ", l.width(), l.height(), -l.width())
                    } else {
                        String::new()
                    },
                    m.x,
                    m.y,
                    m.width,
                    m.height,
                    -m.width
                ),
            )?;
        }
        append_svg(&mut svg, format_args!("</defs>"))?;
        // AE point text and centered contents can have negative source bounds.
        // Legacy box text keeps its fixed mask domain even when animators move ink.
        let mask_bounds = if l.rich_text().is_some_and(|rich| rich.point_origin)
            || matches!(l.content(), Content::ShapeContents(contents) if contents.has_centered_parametrics())
        {
            effect_bounds
        } else {
            [0.0, 0.0, l.width(), l.height()]
        };
        let (path_defs, path_mask) =
            crate::path_mask_render::mask_in_bounds(l, &id, frame, mask_bounds);
        append_svg(&mut svg, format_args!("{path_defs}"))?;
        let a = matrix.0;
        append_svg(
            &mut svg,
            format_args!(
                "<g transform='matrix({} {} {} {} {} {})' opacity='{}'{}>{effect_open}<g {}><g {}><g {path_mask}>",
                a[0],
                a[1],
                a[2],
                a[3],
                a[4],
                a[5],
                l.opacity_at(frame, c.fps().seconds(1))?.clamp(0.0, 100.0) / 100.0,
                match c.compositing_profile() {
                    libre_effects_core::CompositingProfile::NativeV1 => "",
                    libre_effects_core::CompositingProfile::OpaqueOpacityByte257V1 =>
                        " data-libre-effects-compositing='opaque-opacity-byte257-v1'",
                },
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
            ),
        )?;
        let color = format!("#{:06x}", l.color());
        match l.content() {
            Content::Null | Content::Adjustment => {}
            Content::Rectangle | Content::Solid => {
                if generated_spectrum
                    .as_ref()
                    .is_none_or(|(_, original, _)| *original)
                {
                    append_svg(
                        &mut svg,
                        format_args!(
                            "<rect width='{}' height='{}' fill='{color}'/>",
                            l.width(),
                            l.height()
                        ),
                    )?;
                }
                if let Some((_, _, paint)) = &generated_spectrum {
                    append_svg(&mut svg, format_args!("{paint}"))?;
                }
            }
            Content::Shape(shape) => append_svg(
                &mut svg,
                format_args!("{}", shape.svg_at(l.width(), l.height(), l.color(), frame)),
            )?,
            Content::ShapeContents(_) | Content::Text { .. } => {
                append_svg(
                    &mut svg,
                    format_args!("{}", sampled_svg.as_deref().unwrap()),
                )?;
            }
            Content::Image { png } => {
                let png = crate::source_render::alpha_png(png, l.footage_interpretation())?;
                let href = crate::render_images::href(&png)?;
                append_svg(
                    &mut svg,
                    format_args!(
                        "<image width='{}' height='{}' xlink:href='{href}'/>",
                        l.width(),
                        l.height()
                    ),
                )?;
            }
            Content::Composition { composition, .. } => {
                let source = project
                    .composition_by_id(*composition)
                    .ok_or("Missing source composition")?;
                // The containing sample is restored after every recursive call,
                // so hidden mattes and sibling instances retain their own time.
                let parent_sample = sample;
                if let Some(source_sample) = l.composition_sample(parent_sample, c.fps(), source)? {
                    let inner = self.layers_svg_at_sample(
                        project,
                        *composition,
                        source_sample,
                        max_dimension,
                        &id,
                        budget,
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
                        append_svg(&mut svg, format_args!("{inner}"))?;
                    } else {
                        // Nested viewports clip to the source canvas and retain alpha.
                        append_svg(
                            &mut svg,
                            format_args!(
                                "<svg width='{}' height='{}' viewBox='0 0 {} {}' preserveAspectRatio='none' overflow='hidden'>{inner}</svg>",
                                l.width(),
                                l.height(),
                                source.width(),
                                source.height()
                            ),
                        )?;
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
                        let href = crate::render_images::href(&png)?;
                        append_svg(
                            &mut svg,
                            format_args!(
                                "<image width='{}' height='{}' xlink:href='{href}'/>",
                                l.width(),
                                l.height()
                            ),
                        )?;
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
                    let href = crate::render_images::href(&png)?;
                    append_svg(
                        &mut svg,
                        format_args!(
                            "<image width='{}' height='{}' xlink:href='{href}'/>",
                            l.width(),
                            l.height()
                        ),
                    )?;
                }
            }
        }
        append_svg(&mut svg, format_args!("</g></g></g>{effect_close}</g>"))?;

        if let Some((matte, mode)) =
            self.matte_pixels(project, composition, l, frame, max_dimension, &id, budget)?
        {
            let mut source = self.raster_canvas_with_domains(
                &svg,
                c.width(),
                c.height(),
                max_dimension,
                &budget.repeat_domains,
            )?;
            crate::matte_render::apply_matte(&mut source, &matte, mode);
            svg = crate::adjustment_render::embedded(&source, c.width(), c.height())?;
        }
        if svg.len() > SVG_LIMIT {
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
            .map(|frame| frame.pixels)
    }
    pub fn render_preview(
        &self,
        project: &Project,
        frame: u32,
        max_dimension: u32,
    ) -> Result<image::RgbaImage, String> {
        self.render_preview_with_view(project, frame, max_dimension)
            .map(|frame| frame.pixels)
    }
    pub fn render_preview_with_view(
        &self,
        project: &Project,
        frame: u32,
        max_dimension: u32,
    ) -> Result<RenderedFrame, String> {
        self.render_mode(project, frame, max_dimension, true, None)
    }
    /// RAM contains pixels for this exact authored project/frame already. Rebuild
    /// only the root geometry, using the same isolated evaluator and roots as a
    /// full preview. Never retain one evaluated Project per cached frame.
    pub(crate) fn preview_view(
        &self,
        project: &Project,
        frame: u32,
    ) -> Result<Option<Arc<Project>>, String> {
        self.check_cancel()?;
        if frame >= project.composition().duration() {
            return Err("Frame is outside the composition".into());
        }
        let view = self.expression_view(
            project,
            project.active_composition_id(),
            frame,
            true,
            &mut self.frame_budget(),
        )?;
        self.check_cancel()?;
        Ok(view)
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
            .map(|frame| frame.pixels)
    }
    fn render_mode(
        &self,
        project: &Project,
        frame: u32,
        max_dimension: u32,
        include_guides: bool,
        output_size: Option<[u32; 2]>,
    ) -> Result<RenderedFrame, String> {
        let _hardware = crate::gpu_render::begin_frame();
        let _paths = resvg::usvg::install_path_cache(Some(self.paths.clone()));
        let _images = crate::render_images::begin(self.image_sources.clone());
        let _raster = resvg::install_raster_image_cache(Some(self.raster_images.clone()));
        let _filters = resvg::install_filter_cache(Some(self.filter_cache.clone()));
        let _text =
            crate::rich_text_render::install_composition_cache(self.text_compositions.clone());
        crate::gpu_render::reset_timings();
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
        let mut budget = self.frame_budget();
        let root_sample = CompositionSample::from_frame(frame, c.fps())?;
        budget.expression_root = Some((
            project.active_composition_id(),
            root_sample.key(),
            include_guides,
        ));
        let lower_timer = crate::gpu_render::time(crate::gpu_render::Stage::Lowering);
        let body = self.layers_svg(
            project,
            project.active_composition_id(),
            frame,
            max_dimension,
            "root",
            &mut budget,
            include_guides,
        )?;
        let svg = svg_document(&body, f64::from(c.width()), f64::from(c.height()))?;
        drop(lower_timer);
        self.check_cancel()?;
        let parse_timer = crate::gpu_render::time(crate::gpu_render::Stage::Parse);
        let tree = resvg::usvg::Tree::from_str(&svg, &self.options).map_err(|e| e.to_string())?;
        drop(parse_timer);
        self.check_cancel()?;
        let mut pixmap = frame_pixmap(width, height, !budget.repeat_domains.is_empty())?;
        let paint_timer = crate::gpu_render::time(crate::gpu_render::Stage::Paint);
        paint_svg_tree(
            &tree,
            resvg::tiny_skia::Transform::from_scale(
                width as f32 / c.width() as f32,
                height as f32 / c.height() as f32,
            ),
            &mut pixmap.as_mut(),
            &budget.repeat_domains,
        )?;
        drop(paint_timer);
        let finish_timer = crate::gpu_render::time(crate::gpu_render::Stage::Finish);
        let pixels: Vec<u8> = pixmap
            .pixels()
            .iter()
            .flat_map(|p| {
                let p = p.demultiply();
                [p.red(), p.green(), p.blue(), p.alpha()]
            })
            .collect();
        self.check_cancel()?;
        let pixels =
            image::RgbaImage::from_raw(width, height, pixels).ok_or("Invalid render buffer")?;
        drop(finish_timer);
        Ok(RenderedFrame {
            pixels,
            evaluated: budget.expression_view,
        })
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
#[path = "source_text_render_tests.rs"]
pub(crate) mod source_text_tests;

#[cfg(test)]
#[path = "text_opacity_render_tests.rs"]
mod text_opacity_render_tests;

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
    fn temporal_modes_match_preview_output_and_saved_pixels() {
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
        for mode in [
            libre_effects_core::TemporalMode::Auto,
            libre_effects_core::TemporalMode::Continuous,
        ] {
            let before = e.project().clone();
            e.execute(Command::SetTemporalMode {
                id: 1,
                property: Property::PositionX.into(),
                frame: 30,
                mode,
            })
            .unwrap();
            let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            for frame in [15, 45] {
                let output = renderer.render_output(&saved, frame, 200, 120).unwrap();
                assert_eq!(output, renderer.render(e.project(), frame, 200).unwrap());
                assert_ne!(output, renderer.render(&original, frame, 200).unwrap());
            }
            e.undo();
            assert_eq!(e.project(), &before);
            e.redo();
            assert_eq!(e.project(), &saved);
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
    fn opaque_byte_opacity_profile_reopens_renders_and_preserves_partial_alpha_contract() {
        use libre_effects_core::{CompositingProfile, Content, Project, Property};
        let mut editor = scene();
        for (name, color) in [("Gray destination", 0x808080), ("Black foreground", 0)] {
            editor
                .execute(Command::AddContent {
                    content: Content::Solid,
                    width: 100.0,
                    height: 100.0,
                    name: name.into(),
                })
                .unwrap();
            editor
                .execute(Command::SetColor {
                    id: editor.selected().unwrap(),
                    color,
                })
                .unwrap();
        }
        editor
            .execute(Command::SetValue {
                id: 2,
                property: Property::Opacity,
                frame: 0,
                value: 20.0,
            })
            .unwrap();
        let renderer = Renderer::new();
        let original = editor.project().clone();
        let legacy = renderer.render_output(&original, 0, 100, 100).unwrap();
        assert_eq!(legacy.get_pixel(50, 50).0, [102, 102, 102, 255]);
        editor
            .execute(Command::SetCompositingProfile {
                composition: 1,
                profile: CompositingProfile::OpaqueOpacityByte257V1,
            })
            .unwrap();
        let bytes = libre_effects_core::project_file::encode(editor.project(), None).unwrap();
        let reopened = libre_effects_core::project_file::decode(&bytes)
            .unwrap()
            .project;
        let output = renderer.render_output(&reopened, 0, 100, 100).unwrap();
        assert!(output.pixels().all(|p| p.0 == [103, 103, 103, 255]));
        assert_eq!(renderer.render_preview(&reopened, 0, 100).unwrap(), output);
        editor
            .execute(Command::SetValue {
                id: 1,
                property: Property::Opacity,
                frame: 0,
                value: 50.0,
            })
            .unwrap();
        let partial = renderer
            .render_output(editor.project(), 0, 100, 100)
            .unwrap();
        editor
            .execute(Command::SetCompositingProfile {
                composition: 1,
                profile: CompositingProfile::NativeV1,
            })
            .unwrap();
        assert_eq!(
            renderer
                .render_output(editor.project(), 0, 100, 100)
                .unwrap(),
            partial
        );
        assert_eq!(
            Project::from_json(&original.to_json().unwrap()).unwrap(),
            original
        );
        assert_eq!(
            renderer.render_output(&original, 0, 100, 100).unwrap(),
            legacy
        );
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

#[cfg(test)]
#[path = "trim_render_tests.rs"]
pub(crate) mod trim_tests;
