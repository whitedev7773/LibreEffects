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
mod audio_mix;
mod audio_playback;
mod blend_render;
mod cli;
mod color_edit;
mod components;
#[cfg(test)]
mod contents_render_tests;
mod editor;
mod effect_presets;
mod effect_render;
mod font_usage;
mod fonts;
mod footage;
mod image_sequence;
mod matte_render;
mod media_io;
mod output_preflight;
mod output_settings;
mod panels;
mod path_mask_render;
mod preview_cache;
mod project_browser;
mod project_io;
mod recovery;
mod render_queue;
mod rendering;
mod shell;
mod single_instance;
mod source_render;
mod text_edit;
mod text_flow;
mod theme;
#[cfg(test)]
mod time_remap_render;
mod ui;
mod video_decoder;
mod video_export;
mod view_state;
mod viewer_tools;

use shell::Shell;

fn main() {
    #[cfg(target_os = "linux")]
    {
        let is_wsl = std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .is_ok_and(|release| release.to_ascii_lowercase().contains("microsoft"));
        let x11_configured = std::env::var_os("DISPLAY").is_some_and(|display| !display.is_empty());
        let wayland_configured =
            std::env::var_os("WAYLAND_DISPLAY").is_some_and(|display| !display.is_empty());

        if is_wsl && x11_configured && wayland_configured {
            // GPUI 0.2.2 requires xdg_wm_base v2+, while current WSLg advertises v1 and panics.
            // Safety: this is the first statement in `main`, before GPUI or anything
            // else has spawned a thread. No other thread exists yet, so keep this
            // block first if anything is added above it.
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
