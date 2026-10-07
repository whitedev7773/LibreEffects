#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use gpui::{
    App, AppContext, Application, Bounds, SharedString, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};

mod adjustment_render;
mod audio;
mod audio_analysis;
mod audio_mix;
mod audio_playback;
mod audio_selected;
mod audio_spectrum_render;
mod authored_spacing_notice;
mod automation_process;
mod blend_render;
mod build_info;
mod cli;
mod color_edit;
mod components;
#[cfg(test)]
mod contents_render_tests;
mod editor;
mod effect_presets;
mod effect_render;
mod font_coverage;
mod font_usage;
mod fonts;
mod footage;
mod image_sequence;
#[cfg(test)]
mod layer_transform_render_tests;
mod matte_render;
mod media_io;
mod modal_keyboard;
#[cfg(test)]
mod numeric_vertex_render_tests;
#[cfg(test)]
mod opacity_test_support;
mod output_preflight;
mod output_settings;
mod panels;
#[cfg(test)]
mod paragraph_style_guard_review_tests;
mod path_mask_render;
#[cfg(test)]
mod path_order_render_tests;
mod preview_cache;
mod project_browser;
mod project_io;
mod recent_projects;
mod recovery;
mod render_queue;
mod rendering;
mod rich_text_render;
mod shell;
mod single_instance;
mod source_render;
#[cfg(test)]
mod svg_gradient_import_tests;
#[cfg(test)]
mod svg_gradient_reference_tests;
mod svg_import;
#[cfg(test)]
mod svg_import_acceptance_tests;
#[cfg(test)]
mod svg_inline_style_acceptance_tests;
#[cfg(test)]
mod svg_radial_import_tests;
mod text_animator;
mod text_animator_render;
#[cfg(test)]
mod text_animator_stack_acceptance_tests;
mod text_edit;
mod text_flow;
#[cfg(test)]
mod text_paint_render_tests;
mod theme;
#[cfg(test)]
mod time_remap_render;
use libre_effects_editor_model::timeline_filter;
#[cfg(test)]
mod typography_render_tests;
mod ui;
mod video_decoder;
mod video_export;
mod view_state;
mod viewer_tools;
#[cfg(test)]
mod whole_pose_render_tests;

use shell::Shell;

fn main() {
    if let Some(result) = cli::run_worker() {
        if let Err(error) = result {
            eprintln!("JavaScript worker failed: {error}");
            std::process::exit(1);
        }
        return;
    }

    #[cfg(target_os = "linux")]
    {
        let is_wsl = std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .is_ok_and(|release| release.to_ascii_lowercase().contains("microsoft"));
        let x11_configured = std::env::var_os("DISPLAY").is_some_and(|display| !display.is_empty());
        let wayland_configured =
            std::env::var_os("WAYLAND_DISPLAY").is_some_and(|display| !display.is_empty());

        if is_wsl && x11_configured && wayland_configured {
            // GPUI 0.2.2 requires xdg_wm_base v2+, while current WSLg advertises v1 and panics.
            // Safety: this is the first editor initialization, before GPUI or anything
            // else has spawned a thread. No other thread exists yet, so keep this
            // block before any threaded editor initialization.
            unsafe {
                std::env::remove_var("WAYLAND_DISPLAY");
            }
        }
    }

    match cli::run() {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("Render failed: {error}");
            std::process::exit(1);
        }
    }

    let mut instance = match single_instance::Instance::acquire() {
        Ok(Some(instance)) => instance,
        Ok(None) => return,
        Err(error) => {
            single_instance::report_start_error(&error);
            std::process::exit(1);
        }
    };

    Application::new()
        .with_assets(ui::Assets)
        .run(move |cx: &mut App| {
            cx.text_system()
                .add_fonts(vec![std::borrow::Cow::Borrowed(include_bytes!(
                    "../assets/fonts/WantedSans-Regular.ttf"
                ))])
                .expect("load bundled Wanted Sans");
            let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
            let window = cx
                .open_window(
                    WindowOptions {
                        titlebar: Some(TitlebarOptions {
                            title: Some(SharedString::from("Libre Effects")),
                            ..Default::default()
                        }),
                        window_bounds: Some(WindowBounds::Maximized(bounds)),
                        window_min_size: Some(size(px(1100.), px(700.))),
                        ..Default::default()
                    },
                    |window, cx| {
                        modal_keyboard::initialize(window, cx);
                        cx.new(|cx| {
                            cx.observe_window_appearance(window, |_, window, _| {
                                window.refresh();
                            })
                            .detach();

                            Shell::new(cx)
                        })
                    },
                )
                .expect("failed to open the main window");
            cx.spawn(async move |cx| {
                loop {
                    gpui::Timer::after(std::time::Duration::from_millis(250)).await;
                    let (next, activate) = cx
                        .background_executor()
                        .spawn(async move {
                            let activate = instance.take_activation();
                            (instance, activate)
                        })
                        .await;
                    instance = next;
                    if activate
                        && window
                            .update(cx, |shell, window, cx| shell.replace_instance(window, cx))
                            .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        });
}
