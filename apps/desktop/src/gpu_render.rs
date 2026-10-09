//! A frame-local accelerator covers nested SVG/matte preprocessing and final
//! paint. The renderer's authored project and CPU pixel contract are unchanged.
use std::sync::Arc;
#[derive(Clone, Default, serde::Serialize)]
pub(crate) struct Timings {
    lowering_ms: f64,
    parse_ms: f64,
    paint_ms: f64,
    finish_ms: f64,
    expression_ms: f64,
    raster_ms: f64,
    embedded_png_ms: f64,
    video_ms: f64,
    intermediate_cache_hits: u64,
}
thread_local! { static TIMINGS: std::cell::RefCell<Timings> = std::cell::RefCell::new(Timings::default()); }
pub(crate) fn reset_timings() {
    TIMINGS.with(|s| *s.borrow_mut() = Timings::default());
    resvg::reset_render_profile(std::env::var_os("LIBRE_EFFECTS_RENDER_PROFILE").is_some());
}
pub(crate) fn timings() -> Timings {
    TIMINGS.with(|s| s.borrow().clone())
}
pub(crate) fn raster_cache_hit() {
    TIMINGS.with(|s| s.borrow_mut().intermediate_cache_hits += 1);
}
pub(crate) enum Stage {
    Lowering,
    Parse,
    Paint,
    Finish,
    Expression,
    Raster,
    Embedded,
    Video,
}
pub(crate) struct Timer(Stage, std::time::Instant);
pub(crate) fn time(stage: Stage) -> Timer {
    Timer(stage, std::time::Instant::now())
}
impl Drop for Timer {
    fn drop(&mut self) {
        let elapsed = self.1.elapsed().as_secs_f64() * 1000.0;
        TIMINGS.with(|s| {
            let mut s = s.borrow_mut();
            match self.0 {
                Stage::Lowering => s.lowering_ms += elapsed,
                Stage::Parse => s.parse_ms += elapsed,
                Stage::Paint => s.paint_ms += elapsed,
                Stage::Finish => s.finish_ms += elapsed,
                Stage::Expression => s.expression_ms += elapsed,
                Stage::Raster => s.raster_ms += elapsed,
                Stage::Embedded => s.embedded_png_ms += elapsed,
                Stage::Video => s.video_ms += elapsed,
            }
        });
    }
}
struct HardwareBlur;
impl resvg::BoxBlurAccelerator for HardwareBlur {
    fn apply_color_blur(
        &self,
        pixels: &mut [u8],
        width: u32,
        height: u32,
        radii: [[u32; 2]; 5],
        tables: [&[u8; 65536]; 2],
    ) -> bool {
        pixels.len() >= 256 * 1024
            && libre_effects_gpu_render::color_blur(pixels, width, height, radii, tables)
    }
    fn apply_lut(&self, pixels: &mut [u8], width: u32, height: u32, table: &[u8; 65536]) -> bool {
        pixels.len() >= 256 * 1024
            && libre_effects_gpu_render::channel_lut(pixels, width, height, table)
    }
    fn apply_box3(
        &self,
        pixels: &mut [u8],
        width: u32,
        height: u32,
        radii: [f64; 2],
        vertical_first: bool,
    ) -> bool {
        pixels.len() >= 256 * 1024
            && libre_effects_gpu_render::fractional_box3(
                pixels,
                width,
                height,
                radii,
                vertical_first,
            )
    }
    fn apply(&self, pixels: &mut [u8], width: u32, height: u32, radii: [[u32; 2]; 5]) -> bool {
        // Avoid upload/readback overhead for tiny vector/text filters.
        pixels.len() >= 256 * 1024
            && libre_effects_gpu_render::box_blur(pixels, width, height, radii)
    }
}
pub(crate) fn begin_frame() -> resvg::BoxBlurAcceleratorGuard {
    let enabled =
        !cfg!(test) && std::env::var_os("LIBRE_EFFECTS_RENDER_BACKEND").is_none_or(|s| s != "cpu");
    resvg::install_box_blur_accelerator(
        enabled.then(|| Arc::new(HardwareBlur) as Arc<dyn resvg::BoxBlurAccelerator>),
    )
}
