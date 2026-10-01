#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use gpui::{
    App, AppContext, Application, Bounds, SharedString, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};

mod adjustment_render;
mod cli;
mod components;
mod editor;
mod effect_render;
mod footage;
mod media_io;
mod panels;
mod project_io;
mod recovery;
mod rendering;
mod shell;
mod theme;
mod ui;
mod video_export;
mod view_state;

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

    Application::new()
        .with_assets(ui::Assets)
        .run(|cx: &mut App| {
            cx.text_system()
                .add_fonts(vec![std::borrow::Cow::Borrowed(include_bytes!(
                    "../assets/fonts/WantedSans-Regular.ttf"
                ))])
                .expect("load bundled Wanted Sans");
            let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
            cx.open_window(
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
        });
}
