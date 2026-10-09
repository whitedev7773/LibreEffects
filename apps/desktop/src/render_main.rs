//! Console renderer shares the exact editor render/export implementation.
#![allow(dead_code, unused_imports)]
mod adjustment_render;
mod audio;
mod audio_analysis;
mod audio_mix;
mod audio_selected;
mod audio_spectrum_render;
mod automation_process;
mod blend_render;
mod cli;
mod effect_render;
mod fonts;
mod footage;
mod gpu_render;
mod image_sequence;
mod matte_render;
mod media_io;
mod output_preflight;
mod output_settings;
mod path_mask_render;
mod preview_benchmark;
mod preview_cache;
mod preview_frame;
mod project_io;
mod raster_scenes;
mod reference_compare;
mod render_images;
mod render_pipeline;
mod rendering;
mod rich_text_render;
mod source_render;
mod text_animator;
mod text_animator_render;
mod text_flow;
mod text_metrics;
mod video_decoder;
mod video_export;
mod view_state;
mod viewer_tools;

fn main() {
    let result = cli::run_worker().unwrap_or_else(|| {
        cli::run().and_then(|handled| {
            if handled {
                Ok(())
            } else {
                Err("Use --help for file rendering options".into())
            }
        })
    });
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
